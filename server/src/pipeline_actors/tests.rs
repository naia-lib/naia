//! Unit tests for the `pipeline_actors` packaging module.
//!
//! Phase A.1 gate (MISSION_SIM_OWNS_WORLD.md): verify that
//! [`spawn_server_handles`] constructs the three handles cleanly and
//! that each handle is `Send` (cyberlith needs to move them across
//! SubApp thread boundaries).

use naia_shared::Protocol;

use crate::{RecvHandle, SendHandle, ServerConfig};

use super::{CoordHandle, spawn_server_handles};

/// Compile-time assertion that `T: Send`.
fn assert_send<T: Send>() {}

#[test]
fn spawn_server_handles_constructs_three_handles() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (coord, recv, send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

    // The three handles construct and own their own state. The Arc on
    // CoordHandle::shared is the same allocation as on the recv/send
    // handles, so the strong count must be at least 3.
    let strong = std::sync::Arc::strong_count(&coord.shared);
    assert!(
        strong >= 3,
        "expected at least 3 strong refs to ServerShared after split, got {strong}",
    );

    drop((coord, recv, send));
}

#[test]
fn pipeline_handles_are_send() {
    assert_send::<CoordHandle<u64>>();
    assert_send::<RecvHandle<u64>>();
    assert_send::<SendHandle<u64>>();
    // Naia-server doesn't depend on bevy_ecs, so we can't reference
    // `bevy_ecs::entity::Entity` here directly. Cyberlith's
    // `GameCell::init` instantiates `<E = bevy_ecs::entity::Entity>` —
    // that crate-level Send check happens at cyberlith-side compile time
    // via the generic bounds on `spawn_server_handles`.
}
