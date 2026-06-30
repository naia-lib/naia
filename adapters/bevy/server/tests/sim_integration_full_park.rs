//! MISSION_PIPELINE_API_BOUNDARY §2f — park / unpark TestClock integration
//! smoke for `Plugin::pipelined`, driven through the `Server::pipeline_*`
//! static lifecycle helpers (the `PluginInternalState` resource is gone; the
//! pipeline now lives inside the unified `WorldServer` resource).
//!
//! Verifies the D6 discipline ([[project-d6-testclock-findings]]):
//!
//! 1. `Server::pipeline_park` blocks until both worker threads have parked.
//! 2. While parked, the main thread can mutate the shared TestClock via
//!    `TestClock::advance(...)` without racing a worker mid-loop.
//! 3. `Server::pipeline_unpark` resumes both workers.
//! 4. Workers continue to make progress after unpark (no deadlock).
//! 5. Repeated park/unpark cycles work.
//!
//! NOTE: even in the `deterministic` (`workers_active = false`) test build the
//! recv/send worker threads DO spawn (parked-service loop), so `pipeline_park`
//! genuinely blocks until both threads reach their park checkpoint and
//! `pipeline_unpark` resumes them — real worker-lifecycle coverage.

use std::{thread, time::Duration};

use bevy_app::App;

use naia_bevy_server::{transport, PipelineConfig, Plugin as ServerPlugin, Server, ServerConfig};
use naia_bevy_shared::{Protocol as BevyProtocol, TestClock};
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
fn park_workers_returns_promptly() {
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23010");
    app.update();

    // Give workers a moment to spin so they're definitely in their loop body.
    thread::sleep(Duration::from_millis(20));

    let start = std::time::Instant::now();
    Server::pipeline_park(app.world());
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "pipeline_park returned within 2s (took {:?})",
        elapsed,
    );
    Server::pipeline_unpark(app.world());
}

#[test]
fn park_unpark_cycle_progresses_test_clock_safely() {
    // D6 contract: with park around TestClock::advance, the worker reads the new
    // clock value on resume without racing a mid-tick read.
    TestClock::reset();
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23011");
    app.update();

    thread::sleep(Duration::from_millis(20));

    for cycle in 0..5 {
        Server::pipeline_park(app.world());
        // Now safe to mutate the shared clock without racing workers mid-loop.
        TestClock::advance(40);
        Server::pipeline_unpark(app.world());
        // Give workers a bit of unparked time so they see the new clock value.
        thread::sleep(Duration::from_millis(5));
        // If a worker panicked, this re-panics on the main thread.
        Server::pipeline_propagate_panics(app.world());
        let _ = cycle;
    }

    // Final assertion: app drops cleanly (pipeline Drop joins any workers).
    drop(app);
}

#[test]
fn unpark_without_prior_park_is_noop() {
    let app = build_app();
    // Before listen+start the runtime is still Armed (no worker threads yet), so
    // unpark/park are no-ops.
    Server::pipeline_unpark(app.world()); // No panic.
    Server::pipeline_park(app.world()); // No-op too.
}

#[test]
fn sim_handle_borrowable_while_parked() {
    // Demonstrates the cyberlith Phase E pattern: park workers, take the
    // CoordHandle from the pipeline for cross-handle work in a Sim system,
    // restore it, unpark. The pipeline is reached through the unified
    // WorldServer resource via `as_pipelined_mut`.
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23012");
    app.update();

    thread::sleep(Duration::from_millis(10));

    Server::pipeline_park(app.world());
    let borrowed = Server::world_only_resource_scope(app.world_mut(), |_world, ws| {
        if ws.mode() != naia_server::ServerMode::Pipelined {
            return false;
        }
        // Borrow + restore the engine handles while parked.
        let (coord, recv, send) = ws.take_handles();
        ws.restore_handles(coord, recv, send);
        true
    });
    assert!(
        borrowed,
        "pipeline reachable + CoordHandle borrowable while parked"
    );
    Server::pipeline_unpark(app.world());
}
