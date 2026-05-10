//! Supervision tests — RestartPolicy × SupervisionStrategy + restart_limit.
//!
//! Run on multi-thread runtime so spawn paths inside async hooks aren't
//! constrained to current-thread.

use kameo::error::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::message::{Context, Message};
use kameo::supervision::{RestartPolicy, SupervisionStrategy};

// ── Restart-counter child ────────────────────────────────────────────────
//
// Increments a shared atomic on every `on_start` so the test can observe
// how many times the supervisor respawned us.

#[derive(Clone)]
struct CounterArgs {
    starts: Arc<AtomicU32>,
}

struct CrashCounter {
    starts: Arc<AtomicU32>,
}

impl Actor for CrashCounter {
    type Args = CounterArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        args.starts.fetch_add(1, Ordering::SeqCst);
        Ok(CrashCounter { starts: args.starts })
    }
}

struct Boom;
struct ReadStarts;

impl Message<Boom> for CrashCounter {
    type Reply = ();
    async fn handle(&mut self, _msg: Boom, _ctx: &mut Context<Self, Self::Reply>) {
        panic!("boom");
    }
}

impl Message<ReadStarts> for CrashCounter {
    type Reply = u32;
    async fn handle(&mut self, _msg: ReadStarts, _ctx: &mut Context<Self, Self::Reply>) -> u32 {
        self.starts.load(Ordering::SeqCst)
    }
}

// ── Plain supervisor — default OneForOne strategy ─────────────────────────

struct Supervisor;

impl Actor for Supervisor {
    type Args = Self;
    type Error = Infallible;

    async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(args)
    }
}

/// `RestartPolicy::Permanent` (default) restarts the child after a panic.
/// The new instance runs `on_start` again, bumping the starts counter.
#[tokio::test(flavor = "multi_thread")]
async fn restart_policy_permanent_restarts_child_after_panic() {
    let starts = Arc::new(AtomicU32::new(0));
    let supervisor = Supervisor::spawn(Supervisor);

    let child = CrashCounter::supervise(
        &supervisor,
        CounterArgs { starts: starts.clone() },
    )
    .restart_policy(RestartPolicy::Permanent)
    .spawn()
    .await;
    child.wait_for_startup().await;

    // Initial on_start: 1.
    assert_eq!(starts.load(Ordering::SeqCst), 1);

    // Trigger crash via tell so the panic doesn't reach the awaiter.
    child.tell(Boom).await.expect("tell delivered");

    // Wait for the supervisor to respawn the child. Poll the counter.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while starts.load(Ordering::SeqCst) < 2 {
        if tokio::time::Instant::now() > deadline {
            panic!(
                "child not restarted within deadline; starts={}",
                starts.load(Ordering::SeqCst)
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(starts.load(Ordering::SeqCst), 2, "child restarted exactly once");
}

/// `RestartPolicy::Never` does not restart the child after a panic.
#[tokio::test(flavor = "multi_thread")]
async fn restart_policy_never_does_not_restart_on_panic() {
    let starts = Arc::new(AtomicU32::new(0));
    let supervisor = Supervisor::spawn(Supervisor);

    let child = CrashCounter::supervise(
        &supervisor,
        CounterArgs { starts: starts.clone() },
    )
    .restart_policy(RestartPolicy::Never)
    .spawn()
    .await;
    child.wait_for_startup().await;

    assert_eq!(starts.load(Ordering::SeqCst), 1);

    child.tell(Boom).await.expect("tell delivered");

    // Give the supervisor a chance to (not) restart.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(starts.load(Ordering::SeqCst), 1, "child never restarted");
}

/// `restart_limit(n, window)` caps restart storms — after `n` failures within
/// the window, the child is not respawned and the supervisor's
/// `on_link_died` fires (default: stop the supervisor).
#[tokio::test(flavor = "multi_thread")]
async fn restart_limit_caps_storms() {
    let starts = Arc::new(AtomicU32::new(0));
    let supervisor = Supervisor::spawn(Supervisor);

    let child = CrashCounter::supervise(
        &supervisor,
        CounterArgs { starts: starts.clone() },
    )
    .restart_policy(RestartPolicy::Permanent)
    .restart_limit(2, Duration::from_secs(60))
    .spawn()
    .await;
    child.wait_for_startup().await;

    assert_eq!(starts.load(Ordering::SeqCst), 1, "first start");

    // First crash → restart (starts=2).
    child.tell(Boom).await.expect("crash 1 delivered");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Second crash → restart (starts=3); now at the limit.
    let _ = child.tell(Boom).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Third crash → exceeds limit; no further restart.
    let _ = child.tell(Boom).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let final_starts = starts.load(Ordering::SeqCst);
    assert!(
        final_starts <= 3,
        "starts ({final_starts}) exceeded the documented cap"
    );
}
