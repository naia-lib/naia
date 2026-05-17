//! Phase A.2 gate (cyberlith `MISSION_SIM_OWNS_WORLD.md`):
//! public-surface contract test for `naia_server::pipeline_actors`
//! recv-side helpers (`TickMessageRouter`, `RecvLifecycleEvent`,
//! `drain_lifecycle`, `drain_tick_buffer`).
//!
//! Construction pattern mirrors `pipeline_recv_send_independent.rs`:
//! build a `WorldServer<u64>` with a real `LocalServerSocket`, then
//! split via `into_pipeline_handles` to obtain a `RecvHandle<u64>` for
//! the helpers to operate against. Smoke-test verifies the empty path
//! (no connections, no events) — the round-trip "inject + drain"
//! assertion lives in `naia/server/src/pipeline_actors/tests.rs` where
//! crate-private `new_connection_pair` access lets us bypass the full
//! handshake.

#![allow(unused_imports)]

use naia_server::pipeline_actors::{
    self, RecvLifecycleEvent, TickMessageRouter, drain_lifecycle, drain_tick_buffer,
    spawn_server_handles,
};
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};
use naia_server::{ServerConfig, WorldServer};
use naia_shared::Tick;

use naia_test_harness::protocol;

/// Compile-time check that the four public Phase A.2 items are exported
/// from the crate root.
#[allow(dead_code)]
fn _phase_a2_exports_compile() {
    // Trait object should be constructible without naia exposing inner
    // generics — the Send + Sync + 'static bound is what the Recv SubApp
    // needs to store a router as a resource on its thread-pinned world.
    let _box: Option<Box<dyn TickMessageRouter>> = None;
    let _drain: fn(_) -> _ = drain_lifecycle::<u64>;
    let _drain2: fn(_, Tick) -> _ = drain_tick_buffer::<u64>;
    let _variant = RecvLifecycleEvent::RecvError {
        error: naia_server::NaiaServerError::Wrapped(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "compile-time only",
        ))),
    };
    let _ = _variant;
}

const FAKE_SERVER_ADDR: &str = "127.0.0.1:54322";

/// Build a freshly-split `RecvHandle<u64>` backed by a real
/// `LocalServerSocket` (so `RecvHandle::receive` doesn't trip the
/// "must call listen() first" guard). Returns the handle plus the two
/// other split pieces so the caller can keep them alive.
fn build_pipeline_split() -> (
    naia_server::CoordinatorState<u64>,
    naia_server::RecvHandle<u64>,
    naia_server::SendHandle<u64>,
) {
    let mut ws: WorldServer<u64> = WorldServer::new(ServerConfig::default(), protocol());

    let hub = LocalTransportHub::new(FAKE_SERVER_ADDR.parse().unwrap());
    let inner = LocalServerSocket::new(hub);
    let socket = Socket::new(inner, None);
    let (_auth_sender, _auth_receiver, packet_sender, packet_receiver) =
        naia_server::transport::Socket::listen(Box::new(socket));
    ws.io_load(packet_sender, packet_receiver);

    ws.into_pipeline_handles()
}

#[test]
fn drain_tick_buffer_with_no_connections_is_empty() {
    let (_coord, mut recv, _send) = build_pipeline_split();
    // No clients registered → no recv connections → drain returns an
    // empty `TickBufferMessages`. We don't have a public `is_empty` on
    // TickBufferMessages, so the assertion is that the call simply
    // completes and produces a droppable value.
    let messages = drain_tick_buffer(&mut recv, 0);
    drop(messages);
}

#[test]
fn drain_lifecycle_on_freshly_built_recv_handle_is_empty() {
    let (_coord, mut recv, _send) = build_pipeline_split();
    let mut output = recv.receive();
    let events = drain_lifecycle(&mut output);
    assert!(
        events.is_empty(),
        "fresh RecvHandle::receive should produce no lifecycle events: {events:?}",
    );
}

#[test]
fn spawn_server_handles_facade_matches_into_pipeline_handles() {
    // Per Phase A.1, `spawn_server_handles` is a thin facade over
    // `WorldServer::new + into_pipeline_handles`. From the harness side
    // we can confirm it returns the same three-way decomposition that
    // `into_pipeline_handles` does.
    let (coord, recv, send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol());
    // CoordHandle::shared and recv.state.shared / send.state.shared all
    // alias the same Arc<ServerShared>; strong-count must be ≥ 3.
    let strong = std::sync::Arc::strong_count(&coord.shared);
    assert!(strong >= 3, "expected ≥3 strong refs to ServerShared, got {strong}");
    drop((coord, recv, send));
}
