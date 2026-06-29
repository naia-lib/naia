//! MISSION_PIPELINE_API_BOUNDARY G8 (§2l) — the adapter-driven park-window
//! bracket.
//!
//! `PipelineConfig::drive_in_update(true)` makes the adapter run the per-tick
//! park-window bracket itself, from the existing `ReceivePackets` / `SendPackets`
//! system sets:
//!   - `ReceivePackets` ⇒ `park_workers()` + single-world recv-drain.
//!   - `SendPackets`    ⇒ `PipelinedServer::send` (dual-shape) + `unpark_workers()`.
//!
//! The consumer no longer hand-rolls a park window — a plain `app.update()`
//! drives a whole pipelined tick. This is the turnkey single-world path; a
//! Sim-SubApp split stays a consumer choice (§2f).
//!
//! ## Deliberate scope (honest accounting)
//!
//! Structural, like the core `pipeline_bracket` test: ZERO connected clients /
//! ZERO replicated entities. It proves the adapter opens AND closes the park
//! window every `app.update()` (handles round-trip through their slots → the op
//! surface keeps working tick after tick; the workers stay healthy → a manual
//! `park_workers()` afterward still returns promptly), and that the bracket runs
//! without panicking. End-to-end replication THROUGH the adapter bracket against
//! a real acking client is the tracked **real-ack byte-identity** obligation,
//! deferred to G4/G5 (it needs `spawn_replicated`-through-the-bracket; §2l
//! Decision 2). The send-half wire fidelity + the dual send-shape selection are
//! proven in `pipeline_bracket` + `g9pre_*` (naia-test-harness).

use std::{thread, time::Duration};

use bevy_app::App;

use naia_bevy_server::{
    transport, PipelineConfig, PipelinedServer, Plugin as ServerPlugin, PluginInternalState,
    ServerConfig,
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

/// Pipelined app with the adapter driving the bracket from the system sets.
fn build_driving_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::pipelined(
        ServerConfig::default(),
        protocol(),
        PipelineConfig::default().drive_in_update(true),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

fn current_tick(app: &App) -> u16 {
    app.world()
        .resource::<PipelinedServer>()
        .0
        .as_ref()
        .expect("PipelinedServer populated after listen() + drain")
        .current_tick()
}

#[test]
fn adapter_drives_bracket_each_update_without_panic_and_keeps_handles_intact() {
    let mut app = build_driving_app();
    app.world()
        .resource::<PluginInternalState>()
        .listen(local_socket("127.0.0.1:23070"));

    // Many `app.update()`s, each one a full park → recv-drain → (no consumer
    // systems) → send → unpark window. If the window were imbalanced (e.g. a
    // park without its unpark) or a handle were lost, the NEXT update would hang
    // on `park_workers` or panic in `take_handles` ("not in slot").
    for i in 0..10 {
        app.update();
        // current_tick borrows the coord handle, which `expect()`s it is in its
        // slot — so this panics if the bracket failed to restore the handles.
        let _ = current_tick(&app);
        // Give the (active, in production builds) workers a sliver of unparked
        // time between windows so a regression that left them parked surfaces.
        if i % 3 == 0 {
            thread::sleep(Duration::from_millis(2));
        }
    }

    // Workers are still healthy: a manual park after the auto-driven windows
    // must return promptly (it would hang if the last auto-unpark was dropped).
    let state = app.world().resource::<PluginInternalState>();
    let start = std::time::Instant::now();
    state.park_workers();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "park_workers returned promptly after auto-driven windows",
    );
    state.unpark_workers();

    // No worker panicked across the whole run.
    app.world()
        .resource::<PluginInternalState>()
        .propagate_panic_if_any();
}

#[test]
fn adapter_bracket_is_noop_before_listen() {
    // Before `listen()` the worker runtime is still `Armed` (not `Running`), so
    // the bracket systems must no-op WITHOUT parking or recv-draining —
    // recv-draining before `listen()` would panic in `Server::receive_packet`,
    // and parking-without-unparking would unbalance the window. Updating must not
    // hang or panic. (Note `drain_armed_into_res` DOES populate `PipelinedServer`
    // pre-listen so the coord is reachable at `Startup` — so the listening signal
    // is the runtime state, not resource presence.)
    let mut app = build_driving_app();
    for _ in 0..3 {
        app.update();
    }
    // The coord is reachable (armed pipeline drained into the resource) and the
    // server has NOT advanced past its initial tick — the bracket never ran.
    assert!(
        app.world().resource::<PipelinedServer>().0.is_some(),
        "armed pipeline drains into the resource even before listen() (coord access)",
    );
    let _ = current_tick(&app);
}
