//! MISSION_PIPELINE_API_BOUNDARY §2f — Recv + Send worker integration smoke for
//! `Plugin::pipelined`, driven through the `Server::pipeline_*` helpers.
//!
//! With no connected clients the workers loop without producing or consuming
//! meaningful state. These tests verify the pipeline stays healthy across many
//! ticks, that the consumer can drive the send op synchronously inside a park
//! window through the pipeline, and that Drop joins any workers cleanly.
//!
//! NOTE: even in the `deterministic` (`workers_active = false`) test build the
//! recv/send worker threads DO spawn (parked-service loop), so the
//! worker-survival and clean-drop assertions run against real threads; only the
//! send WIRING stays in its inline oracle shape.
//!
//! The old `SnapshotSenderRes` / `SnapshotReceiverRes` / `SendHandleRes`
//! consumer resources are gone (§2f): the snapshot channel is now created
//! INTERNALLY by `PipelinedWorldServer::start_workers`, and the consumer drives
//! the send via `PipelinedWorldServer::send` (reached through the unified
//! WorldServer resource) instead of manually flushing the SendHandle.

use std::time::Duration;

use bevy_app::App;

use naia_bevy_server::{transport, PipelineConfig, Plugin as ServerPlugin, Server, ServerConfig};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxy};
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
    app.add_plugins(ServerPlugin::pipelined(
        ServerConfig::default(),
        protocol(),
        PipelineConfig::default(),
    ));
    app
}

fn local_socket(addr: &str) -> Box<dyn transport::Socket> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    Box::new(Socket::new(LocalServerSocket::new(hub), None))
}

fn listen_and_start(app: &mut App, addr: &str) {
    Server::pipeline_listen(app.world_mut(), local_socket(addr));
    Server::pipeline_start(app.world_mut());
}

#[test]
fn workers_survive_many_ticks_with_no_clients() {
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23030");
    for _ in 0..200 {
        app.update();
    }
    // No worker panicked; propagate is a no-op.
    Server::pipeline_propagate_panics(app.world());
}

#[test]
fn consumer_drives_send_in_park_window() {
    // The consumer-driven park window: park the workers, drive the pipeline's
    // `send` op synchronously against the bevy world, unpark. With no clients
    // and deterministic builds `send` is an inline no-op transmit — the point is
    // that the op routes cleanly through the pipeline and the handles round-trip.
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23031");
    app.update();

    Server::pipeline_park(app.world());
    let sent = Server::world_only_resource_scope(app.world_mut(), |world, ws| {
        if ws.mode() != naia_server::ServerMode::Pipelined {
            return false;
        }
        ws.send(world.proxy());
        true
    });
    Server::pipeline_unpark(app.world());

    assert!(sent, "send routed through the pipeline in the park window");

    Server::pipeline_propagate_panics(app.world());
}

#[test]
fn explicit_shutdown_via_drop_completes_within_5s() {
    // Repeat of the lifecycle drop test but with 200 update cycles in between to
    // verify Drop signaling works even when workers have been running a while.
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23032");
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
