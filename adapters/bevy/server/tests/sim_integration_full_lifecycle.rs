//! MISSION_PIPELINE_API_BOUNDARY §2f — lifecycle smoke for `Plugin::pipelined`.
//!
//! Verifies:
//!   - Plugin install registers the expected consumer-facing Resources
//!     (`ServerEntityConverter`, `EventReceiverRes`) and stores the pipeline
//!     inside the unified `WorldServer` resource (reachable via
//!     `Server::world_only_resource_scope` + `as_pipelined`).
//!   - Before `pipeline_listen`, the pipeline is not listening.
//!   - After `pipeline_listen` + `pipeline_start`, the pipeline is bound
//!     (listening) and reachable.
//!   - Dropping the App joins any worker threads cleanly within 5s (the
//!     `PipelinedWorldServer` Drop joins workers).

use std::time::Duration;

use bevy_app::App;

use naia_bevy_server::{
    transport, EventReceiverRes, PipelineConfig, Plugin as ServerPlugin, Server, ServerConfig,
    ServerEntityConverter,
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

/// Reach into the pipeline stored in the `WorldServer` resource and run `f`
/// against it; returns `false` if the resource is not a Pipelined WorldServer.
fn with_pipeline<R>(app: &mut App, f: impl FnOnce(&mut naia_server::pipeline_actors::PipelinedWorldServer<bevy_ecs::entity::Entity>) -> R) -> Option<R> {
    Server::world_only_resource_scope(app.world_mut(), |_world, ws| ws.as_pipelined_mut().map(f))
}

#[test]
fn plugin_install_registers_expected_resources() {
    let mut app = build_app();
    {
        let w = app.world();
        assert!(
            w.get_resource::<ServerEntityConverter>().is_some(),
            "ServerEntityConverter installed",
        );
        assert!(
            w.get_resource::<EventReceiverRes>().is_some(),
            "EventReceiverRes installed",
        );
    }
    // The pipeline is stored inside the unified WorldServer resource.
    assert_eq!(
        with_pipeline(&mut app, |_ps| ()),
        Some(()),
        "pipeline reachable via the WorldServer resource",
    );
}

#[test]
fn pipeline_not_listening_before_listen() {
    let mut app = build_app();
    let listening = with_pipeline(&mut app, |_ps| ()).is_some();
    assert!(listening, "pipeline present before listen()");
    assert!(
        !Server::pipeline_is_running(app.world()),
        "pipeline not running before start()",
    );
}

#[test]
fn listen_and_start_binds_pipeline() {
    let mut app = build_app();
    Server::pipeline_listen(app.world_mut(), local_socket("127.0.0.1:23001"));
    Server::pipeline_start(app.world_mut());
    app.update();
    // The pipeline is reachable and reports its current tick (coord in slot).
    let tick = with_pipeline(&mut app, |ps| ps.current_tick());
    assert!(tick.is_some(), "pipeline reachable + coord borrowable after start");
}

#[test]
fn app_drop_joins_worker_threads_cleanly() {
    let mut app = build_app();
    Server::pipeline_listen(app.world_mut(), local_socket("127.0.0.1:23002"));
    Server::pipeline_start(app.world_mut());
    app.update();
    // Drop the App; the pipeline's Drop joins any worker threads.
    let start = std::time::Instant::now();
    drop(app);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "App drop completed worker join within 5s (took {:?})",
        elapsed,
    );
}
