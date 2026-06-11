//! MISSION_USER_ONLY_SEES_SIM Phase D.2 + D.3 — Recv + Send worker
//! integration smoke for `Plugin::sim_integration_full`.
//!
//! The two workers are wired in D.1; this file adds explicit
//! integration tests for their per-loop behavior:
//!
//! - **Recv worker** (`recv_worker_loop`) calls `recv.receive()` in a
//!   loop and pushes `ReceiveOutput<Entity>` through a bounded(1)
//!   channel. The main-side `drain_recv_worker_output` system fans
//!   into the bevy `Messages<X>` buffers + `SimEventReceiver`.
//!
//! - **Send worker** (`send_worker_loop`) calls
//!   `SnapshotReceiver::take_latest`, then
//!   `apply_pending_send_preamble` + `apply_pending_scope_changes` +
//!   `send_all_packets` against the drained snapshot.
//!
//! With no connected clients, both workers loop without producing or
//! consuming meaningful state. These tests verify the workers stay
//! alive across many ticks + many snapshot publish/drain cycles, and
//! that `SnapshotSender::send` from main reaches the Send worker
//! (observed via `has_pending` going from `true` to `false`).

use std::{thread, time::Duration};

use bevy_app::App;
use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    transport, Plugin as ServerPlugin, PluginInternalState, PluginSimConfig, SendHandleRes,
    ServerConfig, SnapshotReceiverRes, SnapshotSenderRes,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};
use naia_shared::SnapshotWorld;

use naia_test_harness::test_protocol::Position;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::sim_integration_full(
        ServerConfig::default(),
        protocol(),
        PluginSimConfig::default(),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

#[test]
fn workers_survive_many_ticks_with_no_clients() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23030"));
    }
    for _ in 0..200 {
        app.update();
    }
    // No worker panicked; propagate is a no-op.
    app.world()
        .resource::<PluginInternalState>()
        .propagate_panic_if_any();
}

#[test]
fn published_snapshot_drains_via_consumer_park_window() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23031"));
    }
    app.update();
    // Give worker time to enter its loop.
    thread::sleep(Duration::from_millis(20));

    // Publish an empty snapshot. SnapshotWorld<Entity> needs to be
    // constructible; we use a synthetic empty world.
    let sender = app.world().resource::<SnapshotSenderRes>().0.clone();
    sender.send(SnapshotWorld::<Entity>::new());
    assert!(
        sender.has_pending(),
        "snapshot pending immediately after send"
    );

    // In test_time the send worker is a PURE PARKING SERVICE — it never drains
    // snapshots itself (driving the send on its real-time thread made connect
    // handshakes reorder under parallel load). The consumer (e.g. cyberlith's
    // park window) drives the send synchronously at a deterministic point each
    // tick: park the workers, take the SendHandle from the shared slot, flush
    // the preamble + send the latest snapshot, return the handle, unpark. The
    // drain is therefore synchronous — no wait loop needed.
    {
        let world = app.world();
        let state = world.resource::<PluginInternalState>();
        state.park_workers();
        let send_slot = world.resource::<SendHandleRes>().0.clone();
        let snap = world.resource::<SnapshotReceiverRes>().0.take_latest();
        let mut send = send_slot
            .lock()
            .take()
            .expect("SendHandle in shared slot while workers parked");
        send.apply_pending_send_preamble();
        if let Some(snap) = snap {
            send.send_all_packets(snap);
        }
        *send_slot.lock() = Some(send);
        state.unpark_workers();
    }

    assert!(
        !sender.has_pending(),
        "consumer-driven park-window send drained the published snapshot",
    );

    app.world()
        .resource::<PluginInternalState>()
        .propagate_panic_if_any();
}

#[test]
fn explicit_shutdown_via_drop_completes_within_5s() {
    // Repeat of D.1's app_drop_joins_worker_threads_cleanly but with
    // 200 update cycles in between to verify Drop signaling works
    // even when workers have been running a long time.
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23032"));
    }
    for _ in 0..200 {
        app.update();
    }

    let start = std::time::Instant::now();
    drop(app);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "Drop joined workers within 5s after 200 ticks (took {:?})",
        elapsed,
    );
}
