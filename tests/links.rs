//! Link tests — bidirectional sibling links and `on_link_died`.

use kameo::error::Infallible;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use kameo::actor::{
    ActorId, ActorRef, ActorStateAbsence, ActorTerminalOutcome, ActorTerminalReason, Spawn,
    WeakActorRef,
};
use kameo::error::ActorStopReason;
use kameo::message::{Context, Message};
use kameo::Actor;

/// An actor that records every `on_link_died` invocation.
struct Watcher {
    fired: Arc<AtomicU32>,
    last_outcome: Arc<std::sync::Mutex<Option<ActorTerminalOutcome>>>,
}

impl Actor for Watcher {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(args)
    }

    async fn on_link_died(
        &mut self,
        _ref: WeakActorRef<Self>,
        _id: ActorId,
        outcome: ActorTerminalOutcome,
        reason: ActorStopReason,
    ) -> Result<ControlFlow<ActorStopReason>, Self::Error> {
        self.fired.fetch_add(1, Ordering::SeqCst);
        *self.last_outcome.lock().expect("last outcome lock") = Some(outcome);
        // Stay alive even when peer dies — the test just observes the hook.
        match reason {
            ActorStopReason::Normal | ActorStopReason::SupervisorRestart => {
                Ok(ControlFlow::Continue(()))
            }
            _ => Ok(ControlFlow::Continue(())),
        }
    }
}

struct ReadLastOutcome;
impl Message<ReadLastOutcome> for Watcher {
    type Reply = Option<ActorTerminalOutcome>;
    async fn handle(
        &mut self,
        _msg: ReadLastOutcome,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> Option<ActorTerminalOutcome> {
        *self.last_outcome.lock().expect("last outcome lock")
    }
}

struct ReadFired;
impl Message<ReadFired> for Watcher {
    type Reply = u32;
    async fn handle(&mut self, _msg: ReadFired, _ctx: &mut Context<Self, Self::Reply>) -> u32 {
        self.fired.load(Ordering::SeqCst)
    }
}

/// A simple actor that just exists until killed.
struct Subject;

impl Actor for Subject {
    type Args = Self;
    type Error = Infallible;
    async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(args)
    }
}

/// When two linked actors are paired and one is killed, the survivor's
/// `on_link_died` fires.
#[tokio::test(flavor = "multi_thread")]
async fn linked_peer_death_fires_on_link_died_in_survivor() {
    let fired = Arc::new(AtomicU32::new(0));
    let last_outcome = Arc::new(std::sync::Mutex::new(None));
    let watcher = Watcher::spawn(Watcher {
        fired: fired.clone(),
        last_outcome: last_outcome.clone(),
    });
    let subject = Subject::spawn(Subject);

    watcher.link(&subject).await;

    // Kill the subject; watcher should observe its death via on_link_died.
    subject.kill();
    subject.wait_for_shutdown().await;

    // Give the link-death signal time to traverse.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let observed = watcher.ask(ReadFired).await.expect("read fired count ok");
    assert!(
        observed >= 1,
        "watcher's on_link_died should fire at least once"
    );

    let outcome = watcher
        .ask(ReadLastOutcome)
        .await
        .expect("read last outcome ok")
        .expect("on_link_died records terminal outcome");
    assert_eq!(outcome.state, ActorStateAbsence::Dropped);
    assert_eq!(outcome.reason, ActorTerminalReason::Killed);
}

/// `unlink` removes the link bidirectionally; subsequent peer death does
/// NOT fire the survivor's `on_link_died`.
#[tokio::test(flavor = "multi_thread")]
async fn unlink_prevents_link_death_propagation() {
    let fired = Arc::new(AtomicU32::new(0));
    let last_outcome = Arc::new(std::sync::Mutex::new(None));
    let watcher = Watcher::spawn(Watcher {
        fired: fired.clone(),
        last_outcome: last_outcome.clone(),
    });
    let subject = Subject::spawn(Subject);

    watcher.link(&subject).await;
    watcher.unlink(&subject).await;

    subject.kill();
    subject.wait_for_shutdown().await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let observed = watcher.ask(ReadFired).await.expect("read fired count ok");
    assert_eq!(
        observed, 0,
        "after unlink, peer death must not reach on_link_died"
    );
    assert!(
        watcher
            .ask(ReadLastOutcome)
            .await
            .expect("read last outcome ok")
            .is_none(),
        "after unlink, peer terminal outcome must not reach on_link_died"
    );
}
