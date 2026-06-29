//! G-unify Phase 2c — the unified `WorldServer` enum shell.
//!
//! Proves the one public handle dispatches construction + lifecycle across BOTH
//! engine shapes (resident / pipelined): each variant constructs, reports its
//! `mode()`, binds a real listening socket via the dispatched `listen()`, and
//! answers `current_tick()` through the variant match. The substantive
//! `receive`/`send` drives + `entity_mut` builder arrive in Phase 3/4; this
//! pins the shell so those build on a verified base.

use naia_demo_world::{Entity as DemoEntity, World as DemoWorld};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    ServerConfig, ServerMode, WorldServer,
};
use naia_test_harness::protocol;

fn listening(addr: &str, mut server: WorldServer<DemoEntity>) -> WorldServer<DemoEntity> {
    let hub = LocalTransportHub::new(addr.parse().unwrap());
    let server_socket = ServerSocket::new(LocalServerSocket::new(hub), None);
    server.listen(server_socket);
    server
}

#[test]
fn world_server_enum_resident_shell_dispatches() {
    let server: WorldServer<DemoEntity> = WorldServer::new(ServerConfig::default(), protocol());
    assert_eq!(server.mode(), ServerMode::Resident);
    let mut server = listening("127.0.0.1:54420", server);
    let mut world = DemoWorld::default();

    // The unified receive/send drives dispatch through the Resident arm over
    // several ticks with no clients connected (empty outputs), handles intact.
    for _ in 0..4 {
        let outputs = server.receive(world.proxy_mut());
        assert!(
            outputs.iter().all(|o| o.is_empty()),
            "resident receive with no clients must yield only empty outputs",
        );
        server.send(world.proxy());
        let _tick = server.current_tick();
    }
}

#[test]
fn world_server_enum_pipelined_shell_dispatches() {
    let server: WorldServer<DemoEntity> =
        WorldServer::new_pipelined(ServerConfig::default(), protocol());
    assert_eq!(server.mode(), ServerMode::Pipelined);
    let mut server = listening("127.0.0.1:54421", server);
    let mut world = DemoWorld::default();

    // Same unified surface, dispatched through the Pipelined arm (oracle shape:
    // synchronous bracket, one empty output per receive).
    for _ in 0..4 {
        let outputs = server.receive(world.proxy_mut());
        assert!(
            outputs.iter().all(|o| o.is_empty()),
            "pipelined receive with no clients must yield only empty outputs",
        );
        server.send(world.proxy());
        let _tick = server.current_tick();
    }
}
