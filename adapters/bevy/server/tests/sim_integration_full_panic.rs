//! MISSION_USER_ONLY_SEES_SIM Phase D.5 — panic propagation smoke for
//! `Plugin::sim_integration_full`.
//!
//! Verifies that a panic on a worker thread surfaces on the main
//! thread via [`PluginInternalState::propagate_panic_if_any`].
//!
//! Mechanism: each worker checks `test_panic_request: AtomicBool` at
//! the top of its loop iteration; setting that flag causes the worker
//! to `panic!`, which is captured by `std::panic::catch_unwind` and
//! stashed in `panic_slot`. The main thread polls via
//! `propagate_panic_if_any` (or the `propagate_worker_panics` system
//! that the plugin registers in the change-detection schedule).

use std::{thread, time::Duration};

use bevy_app::App;

use naia_bevy_server::{
    transport, Plugin as ServerPlugin, PluginInternalState, PluginSimConfig, ServerConfig,
};
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
fn worker_panic_surfaces_via_propagate_panic_if_any() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23020"));
    }
    app.update();

    // Request worker panic. Workers panic on their next iteration;
    // `catch_unwind` stashes the payload in `panic_slot`.
    {
        let state = app.world().resource::<PluginInternalState>();
        state.request_worker_panic_for_test();
    }

    // Give workers time to observe + panic.
    thread::sleep(Duration::from_millis(50));

    // propagate_panic_if_any should re-panic on the main thread.
    let state_ref: &PluginInternalState = app.world().resource::<PluginInternalState>();
    let propagate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state_ref.propagate_panic_if_any();
    }));
    assert!(
        propagate.is_err(),
        "propagate_panic_if_any must re-panic when a worker has panicked",
    );

    // App drop is safe even after worker panic.
    drop(app);
}

#[test]
fn no_panic_means_propagate_is_noop() {
    let mut app = build_app();
    {
        let state = app.world().resource::<PluginInternalState>();
        state.listen(local_socket("127.0.0.1:23021"));
    }
    app.update();
    thread::sleep(Duration::from_millis(20));

    // No panic requested → propagate is a no-op.
    app.world()
        .resource::<PluginInternalState>()
        .propagate_panic_if_any();
}
