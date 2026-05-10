//! Registry tests — local `ActorRef::register` / `ActorRef::lookup`.
//!
//! Without `feature = "remote"`, the registry is a process-global
//! `Mutex<HashMap<Cow<'static, str>, RegisteredActorRef>>` at
//! `kameo::registry::ACTOR_REGISTRY`. Both `register` and `lookup` are
//! synchronous in this mode.

use std::convert::Infallible;
use std::time::Duration;

use kameo::Actor;
use kameo::actor::{ActorRef, Spawn};
use kameo::error::RegistryError;
use kameo::message::{Context, Message};

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

/// `register` then `lookup` returns a ref to the same actor.
#[tokio::test]
async fn register_then_lookup_returns_handle_to_same_actor() {
    let name = "kameo_testing::registry::register_then_lookup";
    let actor_ref = Echo::spawn(Echo);
    actor_ref.register(name).expect("register ok");

    let found: Option<ActorRef<Echo>> =
        ActorRef::<Echo>::lookup(name).expect("lookup ok");
    let found = found.expect("lookup returns Some");

    let observed = found.ask(Ping).await.expect("ask via looked-up ref ok");
    assert_eq!(observed, "pong");
}

/// `lookup` for an unknown name returns `Ok(None)`.
#[tokio::test]
async fn lookup_unknown_name_returns_none() {
    let result = ActorRef::<Echo>::lookup("kameo_testing::registry::no_such_name");
    match result {
        Ok(None) => {}
        other => panic!("expected Ok(None), got {other:?}"),
    }
}

/// Registering a name twice (with two actors) returns
/// `RegistryError::NameAlreadyRegistered` on the second attempt; the first
/// registration stays bound to the original actor.
#[tokio::test]
async fn register_collision_returns_name_already_registered() {
    let name = "kameo_testing::registry::collision";
    let first = Echo::spawn(Echo);
    let second = Echo::spawn(Echo);

    first.register(name).expect("first register ok");

    let result = second.register(name);
    match result {
        Err(RegistryError::NameAlreadyRegistered) => {}
        other => panic!("expected NameAlreadyRegistered, got {other:?}"),
    }
}

/// When a registered actor stops, its registry entry is auto-removed
/// (per the 0.19 fix that 0.20 carries forward; see `notes/findings.md`).
#[tokio::test]
async fn registry_entry_auto_removed_on_actor_stop() {
    let name = "kameo_testing::registry::auto_remove_on_stop";
    let actor_ref = Echo::spawn(Echo);
    actor_ref.register(name).expect("register ok");

    actor_ref.kill();
    actor_ref.wait_for_shutdown().await;

    // Give the lifecycle driver a beat to run unregister_actor.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let found = ActorRef::<Echo>::lookup(name).expect("lookup ok");
    assert!(found.is_none(), "entry should be gone after actor stop");
}
