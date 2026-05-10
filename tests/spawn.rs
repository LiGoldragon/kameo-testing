//! Spawn tests — `spawn`, `spawn_with_mailbox`, `prepare/run`,
//! `spawn_in_thread`.

use kameo::error::Infallible;
use std::time::Duration;

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::mailbox;
use kameo::message::{Context, Message};

/// `Spawn::spawn(args)` is synchronous and returns `ActorRef<A>`. The
/// actor runs on the current Tokio runtime.
#[tokio::test]
async fn spawn_returns_actor_ref_synchronously() {
    struct Echo;

    impl Actor for Echo {
        type Args = Self;
        type Error = Infallible;
        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Ping;
    impl Message<Ping> for Echo {
        type Reply = &'static str;
        async fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self, Self::Reply>) -> &'static str {
            "pong"
        }
    }

    // No .await on spawn itself.
    let actor_ref: ActorRef<Echo> = Echo::spawn(Echo);
    assert_eq!(actor_ref.ask(Ping).await.expect("ask ok"), "pong");
}

/// A custom mailbox can be supplied at spawn time.
#[tokio::test]
async fn spawn_with_mailbox_uses_custom_capacity() {
    struct Sink;

    impl Actor for Sink {
        type Args = Self;
        type Error = Infallible;
        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Ping;
    impl Message<Ping> for Sink {
        type Reply = ();
        async fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self, Self::Reply>) {}
    }

    // 4-slot bounded mailbox — small enough to demonstrate sizing.
    let actor_ref = Sink::spawn_with_mailbox(Sink, mailbox::bounded(4));
    actor_ref.ask(Ping).await.expect("ask through small mailbox");
}

/// An unbounded mailbox accepts any number of pending messages
/// without backpressure on `tell`.
#[tokio::test]
async fn unbounded_mailbox_accepts_many_pending_messages() {
    struct Counter { received: u32 }

    impl Actor for Counter {
        type Args = Self;
        type Error = Infallible;
        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Bump;
    struct Read;

    impl Message<Bump> for Counter {
        type Reply = ();
        async fn handle(&mut self, _msg: Bump, _ctx: &mut Context<Self, Self::Reply>) {
            self.received += 1;
        }
    }

    impl Message<Read> for Counter {
        type Reply = u32;
        async fn handle(&mut self, _msg: Read, _ctx: &mut Context<Self, Self::Reply>) -> u32 {
            self.received
        }
    }

    let actor_ref = Counter::spawn_with_mailbox(
        Counter { received: 0 },
        mailbox::unbounded(),
    );

    // Push 1000 tells without awaiting; an unbounded mailbox should
    // accept them all without blocking.
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        actor_ref.tell(Bump).await.expect("tell to unbounded ok");
    }
    let tell_elapsed = start.elapsed();
    assert!(
        tell_elapsed < Duration::from_millis(500),
        "1000 tells to unbounded mailbox took {tell_elapsed:?}"
    );

    // Then ask — by the time it replies, all 1000 bumps have been
    // processed (mailbox is FIFO).
    let observed = actor_ref.ask(Read).await.expect("read ok");
    assert_eq!(observed, 1000);
}
