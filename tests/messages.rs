//! Message tests — per-kind `Message<T>` impls, replies, DelegatedReply.

use kameo::error::Infallible;
use std::time::Duration;

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::error::SendError;
use kameo::message::{Context, Message};
use kameo::reply::DelegatedReply;

/// One actor can implement `Message<T>` for many distinct request
/// types; each impl carries its own `Reply` type.
#[tokio::test]
async fn multiple_message_impls_compose_on_one_actor() {
    struct Calculator { acc: i64 }

    impl Actor for Calculator {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Add { value: i64 }
    struct Multiply { value: i64 }
    struct Read;

    impl Message<Add> for Calculator {
        type Reply = i64;
        async fn handle(&mut self, msg: Add, _ctx: &mut Context<Self, Self::Reply>) -> i64 {
            self.acc += msg.value;
            self.acc
        }
    }

    impl Message<Multiply> for Calculator {
        type Reply = i64;
        async fn handle(&mut self, msg: Multiply, _ctx: &mut Context<Self, Self::Reply>) -> i64 {
            self.acc *= msg.value;
            self.acc
        }
    }

    impl Message<Read> for Calculator {
        type Reply = i64;
        async fn handle(&mut self, _msg: Read, _ctx: &mut Context<Self, Self::Reply>) -> i64 {
            self.acc
        }
    }

    let actor_ref = Calculator::spawn(Calculator { acc: 0 });
    let _ = actor_ref.ask(Add { value: 7 }).await.expect("add ok");
    let _ = actor_ref.ask(Multiply { value: 3 }).await.expect("multiply ok");
    let observed = actor_ref.ask(Read).await.expect("read ok");
    assert_eq!(observed, 21, "(0 + 7) * 3 = 21");
}

/// `tell` does not await the reply. The handler still runs; the
/// caller continues immediately.
#[tokio::test]
async fn tell_does_not_await_reply() {
    struct Sink;

    impl Actor for Sink {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Slow;
    struct Done;

    impl Message<Slow> for Sink {
        type Reply = ();
        async fn handle(&mut self, _msg: Slow, _ctx: &mut Context<Self, Self::Reply>) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    impl Message<Done> for Sink {
        type Reply = ();
        async fn handle(&mut self, _msg: Done, _ctx: &mut Context<Self, Self::Reply>) {}
    }

    let actor_ref = Sink::spawn(Sink);
    let start = std::time::Instant::now();
    actor_ref.tell(Slow).await.expect("tell delivered");
    let tell_elapsed = start.elapsed();
    // tell should return well before the 50ms handler completes.
    assert!(
        tell_elapsed < Duration::from_millis(40),
        "tell returned in {tell_elapsed:?}, expected < 40ms"
    );
    // ask, by contrast, would wait — verify the actor is still alive.
    actor_ref.ask(Done).await.expect("ask after tell still works");
}

/// A handler returning `Result<T, E>` propagates Err to the asker,
/// so callers can pattern-match on typed failure.
#[tokio::test]
async fn result_reply_propagates_handler_error_to_caller() {
    #[derive(Debug, PartialEq, Eq)]
    enum DivisionError { ByZero }

    struct Divider;

    impl Actor for Divider {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Divide { num: i64, den: i64 }

    impl Message<Divide> for Divider {
        type Reply = Result<i64, DivisionError>;
        async fn handle(&mut self, msg: Divide, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
            if msg.den == 0 {
                Err(DivisionError::ByZero)
            } else {
                Ok(msg.num / msg.den)
            }
        }
    }

    let actor_ref = Divider::spawn(Divider);

    // ask().await unwraps Reply::Value: a Result-typed handler that returns
    // Ok(T) gives the caller Ok(T) directly; Err(E) is wrapped as
    // SendError::HandlerError(E).
    let ok_value = actor_ref.ask(Divide { num: 10, den: 2 }).await.expect("ok divide");
    assert_eq!(ok_value, 5);

    let err = actor_ref.ask(Divide { num: 10, den: 0 }).await;
    match err {
        Err(SendError::HandlerError(DivisionError::ByZero)) => {}
        other => panic!("expected HandlerError(ByZero), got {other:?}"),
    }
}

/// `DelegatedReply` lets a handler defer the reply to a spawned task
/// without blocking the actor's mailbox. The sender of the message
/// awaits as normal; the handler returns immediately so the next
/// message in the mailbox can be processed.
#[tokio::test]
async fn delegated_reply_defers_response_to_spawned_task() {
    struct Worker;

    impl Actor for Worker {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct DoSlow { delay_ms: u64 }
    struct Ping;

    impl Message<DoSlow> for Worker {
        type Reply = DelegatedReply<String>;

        async fn handle(
            &mut self,
            msg: DoSlow,
            ctx: &mut Context<Self, Self::Reply>,
        ) -> Self::Reply {
            let (delegated, sender) = ctx.reply_sender();
            if let Some(tx) = sender {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(msg.delay_ms)).await;
                    tx.send(format!("delayed {}ms", msg.delay_ms));
                });
            }
            delegated
        }
    }

    impl Message<Ping> for Worker {
        type Reply = u32;
        async fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self, Self::Reply>) -> u32 {
            42
        }
    }

    let actor_ref = Worker::spawn(Worker);

    // Kick off a slow ask in a separate task; meanwhile the actor's
    // mailbox should still be responsive to other messages because
    // DoSlow's handler returned immediately.
    let slow_ref = actor_ref.clone();
    let slow_task = tokio::spawn(async move {
        slow_ref.ask(DoSlow { delay_ms: 100 }).await.expect("slow ask ok")
    });

    // While the slow task waits, ask Ping — must respond promptly.
    let ping_start = std::time::Instant::now();
    let ping_result = actor_ref.ask(Ping).await.expect("ping ok");
    let ping_elapsed = ping_start.elapsed();
    assert_eq!(ping_result, 42);
    assert!(
        ping_elapsed < Duration::from_millis(50),
        "ping responded in {ping_elapsed:?}; mailbox was blocked by DelegatedReply"
    );

    let slow_reply = slow_task.await.expect("slow task joins");
    assert_eq!(slow_reply, "delayed 100ms");
}
