//! MISSION_PIPELINE_API_BOUNDARY G8 (§2l) + §2f — the adapter-driven park-window
//! bracket.
//!
//! `PipelineConfig::drive_in_update(true)` makes the adapter run the per-tick
//! park-window bracket itself, from the existing `ReceivePackets` / `SendPackets`
//! system sets:
//!   - `ReceivePackets` ⇒ `PipelinedWorldServer::receive` (parks internally).
//!   - `SendPackets`    ⇒ `PipelinedWorldServer::send` (unparks internally).
//!
//! The consumer no longer hand-rolls a park window — a plain `app.update()`
//! drives a whole pipelined tick. The pipeline lives inside the unified
//! `WorldServer` resource (§2f), reached via `Server::world_only_resource_scope`.
//!
//! ## Deliberate scope
//!
//! Structural: ZERO connected clients / ZERO replicated entities. It proves the
//! adapter opens AND closes the park window every `app.update()` and runs the
//! bracket without panicking. The worker threads spawn even in the deterministic
//! test build (parked-service loop), so the bracket drives real park/unpark each
//! update and the op surface round-trips tick after tick.

use std::{thread, time::Duration};

use bevy_app::App;

use naia_bevy_server::{transport, PipelineConfig, Plugin as ServerPlugin, Server, ServerConfig};
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
    app.add_plugins(ServerPlugin::new(
        naia_bevy_server::ServerPluginConfig::new(
            ServerConfig::default(),
            protocol(),
            naia_bevy_server::Topology::WorldProxied(naia_bevy_server::DriveShape::Pipelined(
                PipelineConfig::default().drive_in_update(true),
            )),
        ),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

/// Borrow the pipeline's current tick — `expect`s the coord handle is back in
/// its slot, so this panics if the bracket failed to restore the handles.
fn current_tick(app: &mut App) -> u16 {
    Server::world_only_resource_scope(app.world_mut(), |_world, ws| ws.current_tick())
}

#[test]
fn adapter_drives_bracket_each_update_without_panic_and_keeps_handles_intact() {
    let mut app = build_driving_app();
    Server::pipeline_listen(app.world_mut(), local_socket("127.0.0.1:23070"));
    Server::pipeline_start(app.world_mut());

    // Many `app.update()`s, each one a full park → recv-drain → (no consumer
    // systems) → send → unpark window. If the window were imbalanced or a handle
    // were lost, the NEXT update / coord borrow would hang or panic.
    for i in 0..10 {
        app.update();
        let _: u16 = current_tick(&mut app);
        if i % 3 == 0 {
            thread::sleep(Duration::from_millis(2));
        }
    }

    // Workers are still healthy: a manual park after the auto-driven windows must
    // return promptly (it would hang if the last auto-unpark was dropped).
    let start = std::time::Instant::now();
    Server::pipeline_park(app.world());
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "pipeline_park returned promptly after auto-driven windows",
    );
    Server::pipeline_unpark(app.world());

    // No worker panicked across the whole run.
    Server::pipeline_propagate_panics(app.world());
}

#[test]
fn adapter_bracket_is_noop_before_listen() {
    // Before `listen()`/`start()` the worker runtime is not `Running`, so the
    // bracket systems must no-op WITHOUT parking or recv-draining (recv-draining
    // before listen would panic in `Server::receive_packet`). Updating must not
    // hang or panic, and the pipeline must remain reachable.
    let mut app = build_driving_app();
    for _ in 0..3 {
        app.update();
    }
    assert!(
        !Server::pipeline_is_running(app.world()),
        "runtime not Running before start()",
    );
    let _: u16 = current_tick(&mut app);
}
