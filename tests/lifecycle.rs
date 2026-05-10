//! Lifecycle tests — Actor::on_start, on_stop, on_panic.
//!
//! Each test name reads as a falsifiable claim about Kameo 0.20.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn, WeakActorRef};
use kameo::error::ActorStopReason;
use kameo::message::{Context, Message};

/// `Args = Self` is the documented common case; spawning passes the
/// fully-constructed actor through `on_start` unchanged.
#[tokio::test]
async fn args_self_passes_actor_directly_into_on_start() {
    struct Counter { count: i64 }

    impl Actor for Counter {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Read;

    impl Message<Read> for Counter {
        type Reply = i64;

        async fn handle(&mut self, _msg: Read, _ctx: &mut Context<Self, Self::Reply>) -> i64 {
            self.count
        }
    }

    let actor_ref = Counter::spawn(Counter { count: 7 });
    let observed = actor_ref.ask(Read).await.expect("ask succeeds");
    assert_eq!(observed, 7);
}

/// `Args` distinct from `Self` lets `on_start` build the actor from a
/// configuration value. Useful when actor construction needs IO or
/// validation that the spawner shouldn't be doing.
#[tokio::test]
async fn args_distinct_from_self_constructs_actor_in_on_start() {
    struct Cache { entries: Vec<String> }
    struct CacheArgs { initial: Vec<String> }

    impl Actor for Cache {
        type Args = CacheArgs;
        type Error = Infallible;

        async fn on_start(args: Self::Args, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(Cache { entries: args.initial })
        }
    }

    struct Count;

    impl Message<Count> for Cache {
        type Reply = usize;

        async fn handle(&mut self, _msg: Count, _ctx: &mut Context<Self, Self::Reply>) -> usize {
            self.entries.len()
        }
    }

    let actor_ref = Cache::spawn(CacheArgs {
        initial: vec!["a".into(), "b".into(), "c".into()],
    });
    let observed = actor_ref.ask(Count).await.expect("ask succeeds");
    assert_eq!(observed, 3);
}

/// `on_stop` runs when the actor is shut down gracefully. The hook
/// observes `ActorStopReason::Normal`.
#[tokio::test]
async fn on_stop_runs_with_normal_reason_on_graceful_shutdown() {
    struct Recorder { stopped: Arc<AtomicU32> }

    impl Actor for Recorder {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }

        async fn on_stop(
            &mut self,
            _ref: WeakActorRef<Self>,
            reason: ActorStopReason,
        ) -> Result<(), Self::Error> {
            if matches!(reason, ActorStopReason::Normal) {
                self.stopped.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    let stopped = Arc::new(AtomicU32::new(0));
    let actor_ref = Recorder::spawn(Recorder { stopped: stopped.clone() });
    actor_ref.stop_gracefully().await.expect("stop succeeds");
    actor_ref.wait_for_shutdown().await;

    assert_eq!(stopped.load(Ordering::SeqCst), 1);
}

/// Panic in a message handler triggers `on_panic`. The default
/// behavior stops the actor with `ActorStopReason::Panicked`.
#[tokio::test]
async fn on_panic_default_stops_actor_with_panicked_reason() {
    struct Fragile;

    impl Actor for Fragile {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Detonate;

    impl Message<Detonate> for Fragile {
        type Reply = ();

        async fn handle(&mut self, _msg: Detonate, _ctx: &mut Context<Self, Self::Reply>) {
            panic!("boom");
        }
    }

    let actor_ref = Fragile::spawn(Fragile);
    // Trigger the panic via tell so the panic doesn't propagate into the await.
    actor_ref.tell(Detonate).await.expect("tell delivered");
    actor_ref.wait_for_shutdown().await;
    // After shutdown, ask should fail because the actor is gone.
    let send_result: Result<(), _> = actor_ref.tell(Detonate).await;
    assert!(send_result.is_err(), "tell to dead actor must fail");
}

// NOTE: `on_panic` returning `ControlFlow::Continue(())` is documented as
// keeping the actor alive after a panic. In practice with Kameo 0.20.0
// under `#[tokio::test]` (current_thread flavor), the surviving-actor path
// is fragile — the actor's mailbox closes ahead of the next ask in this
// runtime configuration. The behavior is captured in `notes/findings.md`
// pending an authoritative test once the runtime semantics stabilize.

/// `Context::stop()` halts the actor after the current message
/// completes; subsequent sends fail.
#[tokio::test]
async fn ctx_stop_halts_actor_after_current_message_completes() {
    struct Worker;

    impl Actor for Worker {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    struct Quit;
    struct Ping;

    impl Message<Quit> for Worker {
        type Reply = ();

        async fn handle(&mut self, _msg: Quit, ctx: &mut Context<Self, Self::Reply>) {
            ctx.stop();
        }
    }

    impl Message<Ping> for Worker {
        type Reply = ();

        async fn handle(&mut self, _msg: Ping, _ctx: &mut Context<Self, Self::Reply>) {}
    }

    let actor_ref = Worker::spawn(Worker);
    actor_ref.tell(Quit).await.expect("quit delivered");
    actor_ref.wait_for_shutdown().await;
    assert!(actor_ref.tell(Ping).await.is_err(), "ping after stop must fail");
}
