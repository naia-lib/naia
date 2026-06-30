//! MISSION_PIPELINE_API_BOUNDARY §2f — panic propagation smoke for
//! `Plugin::pipelined`, driven through the `Server::pipeline_*` static helpers.
//!
//! Verifies that a panic on a worker thread surfaces on the main thread via
//! [`naia_bevy_server::Server::pipeline_propagate_panics`].
//!
//! NOTE: even in the `deterministic` (`workers_active = false`) test build the
//! recv/send worker threads DO spawn — they run the parked-service loop, so
//! park/unpark/panic-capture/join are all real. Panic propagation is therefore
//! exercised for real here (`is_running()` is `true` after listen+start).

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
fn worker_panic_surfaces_via_propagate_panic_if_any() {
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23020");
    app.update();

    // The worker threads spawn even in the deterministic test build (parked
    // service loop), so the runtime is genuinely Running here.
    assert!(
        Server::pipeline_is_running(app.world()),
        "worker runtime Running after listen+start",
    );

    // Request worker panic. A worker panics on its next iteration; `catch_unwind`
    // stashes the payload in the runtime's `panic_slot`.
    Server::pipeline_request_worker_panic_for_test(app.world());

    // Give workers time to observe + panic.
    thread::sleep(Duration::from_millis(50));

    // Propagation must re-panic on the main thread.
    let propagate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Server::pipeline_propagate_panics(app.world());
    }));
    assert!(
        propagate.is_err(),
        "pipeline_propagate_panics must re-panic when a worker has panicked",
    );

    // App drop is safe even after a worker panic.
    drop(app);
}

#[test]
fn no_panic_means_propagate_is_noop() {
    let mut app = build_app();
    listen_and_start(&mut app, "127.0.0.1:23021");
    app.update();
    thread::sleep(Duration::from_millis(20));

    // No panic requested → propagate is a no-op.
    Server::pipeline_propagate_panics(app.world());
}
