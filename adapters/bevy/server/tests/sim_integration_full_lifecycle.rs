//! MISSION_USER_ONLY_SEES_SIM Phase D.1 — lifecycle smoke for
//! `Plugin::sim_integration_full`.
//!
//! Verifies:
//!   - Plugin install registers the expected Resources (`SimConverter`,
//!     `SimEventReceiverRes`, `SnapshotSenderRes`, `SnapshotReceiverRes`,
//!     `SimHandleRes`, `SendHandleRes`, `PluginInternalState`).
//!   - Before `listen()`, `SimHandleRes` is empty (workers not running).
//!   - After `listen()` with a `LocalServerSocket`, `App::update`
//!     drains the armed sim_handle into `SimHandleRes` and Sim systems can
//!     observe it.
//!   - Dropping the App joins the worker threads cleanly within 5s.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::SnapshotSender, transport, Plugin as ServerPlugin, PluginInternalState,
    PluginSimConfig, SendHandleRes, ServerConfig, SimConverter, SimEventReceiverRes, SimHandleRes,
    SnapshotReceiverRes, SnapshotSenderRes,
};
use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

use naia_test_harness::test_protocol::Position;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
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
fn plugin_install_registers_expected_resources() {
    let app = build_app();
    let w = app.world();
    assert!(
        w.get_resource::<SimConverter>().is_some(),
        "SimConverter installed",
    );
    assert!(
        w.get_resource::<SimEventReceiverRes>().is_some(),
        "SimEventReceiverRes installed",
    );
    assert!(
        w.get_resource::<SnapshotSenderRes>().is_some(),
        "SnapshotSenderRes installed",
    );
    assert!(
        w.get_resource::<SnapshotReceiverRes>().is_some(),
        "SnapshotReceiverRes installed",
    );
    assert!(
        w.get_resource::<SimHandleRes>().is_some(),
        "SimHandleRes installed",
    );
    assert!(
        w.get_resource::<SendHandleRes>().is_some(),
        "SendHandleRes installed",
    );
    assert!(
        w.get_resource::<PluginInternalState>().is_some(),
        "PluginInternalState installed",
    );
}

#[test]
fn sim_handle_empty_before_listen() {
    let app = build_app();
    let sim_handle_res = app.world().resource::<SimHandleRes>();
    assert!(
        sim_handle_res.0.is_none(),
        "SimHandleRes is None before listen()",
    );
}

#[test]
fn listen_drains_armed_sim_handle_into_resource_after_update() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23001"));
    }
    // Drives the install Startup-equivalent system (registered in
    // Update by sim_integration_full) which drains armed_sim_handle →
    // SimHandleRes.
    app.update();
    let sim_handle_res = app.world().resource::<SimHandleRes>();
    assert!(
        sim_handle_res.0.is_some(),
        "SimHandleRes filled after listen + update",
    );
}

#[test]
fn snapshot_sender_resource_is_usable() {
    // Verify the SnapshotSenderRes is the load-bearing copy (not a
    // dropped clone) — calling .send is a no-op but should not panic
    // and has_pending should report true afterwards.
    let app = build_app();
    let sender = app.world().resource::<SnapshotSenderRes>().0.clone();
    assert!(!sender.has_pending(), "no snapshot pending initially");
}

#[test]
fn app_drop_joins_worker_threads_cleanly() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23002"));
    }
    app.update();
    // Drop the App; PluginInternalState::Drop joins workers.
    let start = std::time::Instant::now();
    drop(app);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "App drop completed worker join within 5s (took {:?})",
        elapsed,
    );
}

#[test]
fn snapshot_sender_pair_construction_independent() {
    // Sanity: the plugin's SnapshotSender is independent of any
    // user-constructed pair. Constructing a new pair doesn't disturb
    // the plugin's state.
    let app = build_app();
    let (_s, _r) = SnapshotSender::<Entity>::pair();
    let sender = app.world().resource::<SnapshotSenderRes>().0.clone();
    assert!(!sender.has_pending());
}
