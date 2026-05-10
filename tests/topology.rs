//! Topology / workspace witness tests.
//!
//! These tests prove that Kameo's actor shape *agrees* with this
//! workspace's `skills/actor-systems.md` no-ZST rule. They are not
//! testing Kameo's behavior; they are testing that an actor written
//! the workspace way *fits* Kameo natively.

use std::convert::Infallible;
use std::mem::size_of;

use kameo::actor::{ActorRef, Spawn};
use kameo::Actor;

/// The workspace rule: every meaningful actor noun carries data in
/// its own type. Kameo's native shape (`Args = Self`, `Self IS the
/// state`) makes this a runtime-checkable invariant: a data-bearing
/// actor type has nonzero size.
#[tokio::test]
async fn data_bearing_actor_types_have_nonzero_size() {
    #[allow(dead_code)]
    struct ClaimNormalize {
        in_flight: Vec<String>,
        max_in_flight: usize,
        normalize_count: u64,
    }

    impl Actor for ClaimNormalize {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    // The actor type IS the state; its size reflects its fields.
    assert!(
        size_of::<ClaimNormalize>() > 0,
        "actor type should carry data, not be a ZST namespace"
    );

    let actor_ref = ClaimNormalize::spawn(ClaimNormalize {
        in_flight: Vec::new(),
        max_in_flight: 64,
        normalize_count: 0,
    });
    drop(actor_ref);
}

/// A unit-struct actor is technically legal Rust, but the workspace
/// rule says public actor nouns must not be hollow markers. This
/// test documents the shape Kameo allows AND that workspace code
/// must avoid for *public* actor types.
///
/// Kameo doesn't enforce non-zero size; the workspace does, by
/// convention enforced in code review and skills/kameo.md.
#[tokio::test]
async fn kameo_permits_zst_actors_but_workspace_rejects_them_by_convention() {
    struct PingMarker;

    impl Actor for PingMarker {
        type Args = Self;
        type Error = Infallible;

        async fn on_start(args: Self, _ref: ActorRef<Self>) -> Result<Self, Self::Error> {
            Ok(args)
        }
    }

    // Kameo lets it compile and spawn.
    assert_eq!(size_of::<PingMarker>(), 0, "ZST actor compiles");
    let actor_ref = PingMarker::spawn(PingMarker);
    drop(actor_ref);

    // Workspace rule: public actor nouns must carry data. This test's
    // existence is the reminder; enforcement is in the skill, not
    // the framework.
}
