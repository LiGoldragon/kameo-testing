//! Mailbox tests — bounded backpressure, unbounded, default capacity.
//!
//! Default mailbox is bounded with `DEFAULT_MAILBOX_CAPACITY = 64` per
//! `kameo::actor` (private constant; named in `notes/findings.md`).

use std::convert::Infallible;
use std::time::Duration;

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::error::SendError;
use kameo::mailbox;
use kameo::message::{Context, Message};

struct SlowSink;

impl Actor for SlowSink {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(args)
    }
}

struct Hold;

impl Message<Hold> for SlowSink {
    type Reply = ();
    async fn handle(&mut self, _msg: Hold, _ctx: &mut Context<Self, Self::Reply>) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// `try_send` on a saturated bounded mailbox returns `MailboxFull` instead
/// of waiting. We saturate by enqueueing more than capacity while the
/// handler holds the loop.
#[tokio::test]
async fn bounded_full_try_send_returns_mailbox_full() {
    // Capacity 1: one message in-flight (handler awaiting sleep), one
    // queued; the third try_send must surface MailboxFull.
    let actor_ref = SlowSink::spawn_with_mailbox(SlowSink, mailbox::bounded(1));

    // Kick off the slow handler.
    actor_ref.tell(Hold).await.expect("first tell delivered");

    // Wait briefly to ensure the first message is being processed.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Enqueue a second — fills the slot.
    actor_ref.tell(Hold).await.expect("second tell queued");

    // Third tell via try_send — must fail with MailboxFull.
    let result = actor_ref.tell(Hold).try_send();
    match result {
        Err(SendError::MailboxFull(_)) => {}
        other => panic!("expected MailboxFull, got {other:?}"),
    }
}

/// `tell` to a closed (stopped) actor returns `ActorNotRunning(msg)`.
#[tokio::test]
async fn tell_to_stopped_actor_returns_actor_not_running() {
    let actor_ref = SlowSink::spawn(SlowSink);
    actor_ref.kill();
    actor_ref.wait_for_shutdown().await;

    let result = actor_ref.tell(Hold).await;
    match result {
        Err(SendError::ActorNotRunning(_)) => {}
        other => panic!("expected ActorNotRunning, got {other:?}"),
    }
}

/// `mailbox::unbounded()` accepts any number of pending messages without
/// blocking on `tell`.
#[tokio::test]
async fn unbounded_mailbox_accepts_burst_without_blocking() {
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

    let start = std::time::Instant::now();
    for _ in 0..500 {
        actor_ref.tell(Bump).await.expect("tell to unbounded ok");
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "500 tells to unbounded mailbox took {elapsed:?}"
    );

    let observed = actor_ref.ask(Read).await.expect("read ok");
    assert_eq!(observed, 500);
}
