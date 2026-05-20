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
    transport, Plugin as ServerPlugin, PluginInternalState, PluginSimConfig, ServerConfig,
    SnapshotSenderRes,
};
use naia_shared::SnapshotWorld;
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

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
fn send_worker_drains_published_snapshots() {
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
    assert!(sender.has_pending(), "snapshot pending immediately after send");

    // Under the E.6 zero-CPU-idle worker model, an idle (body-sleeping) send
    // worker does not poll for snapshots out-of-band in test_time mode — it
    // drains during a park window. Drive one park/unpark cycle so the worker
    // wakes, observes the published snapshot via take_latest, and drains it.
    // (Production / non-test_time workers poll autonomously every ~100µs, but
    // that path is unreachable here: this crate's dev-dependency forces the
    // test_time feature on, so these tests always run park-driven.)
    {
        let state = app.world().resource::<PluginInternalState>();
        state.park_workers();
        state.unpark_workers();
    }

    // Give the Send worker time to drain.
    let drain_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while sender.has_pending() && std::time::Instant::now() < drain_deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !sender.has_pending(),
        "Send worker drained the published snapshot within 2s",
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
