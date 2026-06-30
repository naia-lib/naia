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
    EntityOwner, ReplicationConfig, ServerConfig, ServerMode, WorldServer,
};
use naia_shared::WorldMutType;
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

/// The imperative entity builder dispatches through both engine shapes. The
/// Pipelined arm exercises the coord-only fast paths (`enable_replication`,
/// `configure_replication`) on a fresh (never-started) pipelined server.
fn drive_entity_builder(mut server: WorldServer<DemoEntity>) {
    let mut world = DemoWorld::default();
    let entity = world.proxy_mut().spawn_entity();

    // server.entity_mut(e).enable_replication().configure_replication(cfg)
    server
        .entity_mut(world.proxy_mut(), &entity)
        .enable_replication()
        .configure_replication(ReplicationConfig::public());

    // Reads dispatch through the same builder: enabling a server entity makes
    // it Server-owned and registers a replication config.
    let em = server.entity_mut(world.proxy_mut(), &entity);
    assert_eq!(em.owner(), EntityOwner::Server, "enabled entity must be Server-owned");
    assert!(
        em.replication_config().is_some(),
        "configured entity must report a replication config",
    );
}

#[test]
fn world_server_enum_resident_entity_builder() {
    drive_entity_builder(WorldServer::new(ServerConfig::default(), protocol()));
}

#[test]
fn world_server_enum_pipelined_entity_builder() {
    drive_entity_builder(WorldServer::new_pipelined(ServerConfig::default(), protocol()));
}

/// task #9 — the Room/global-priority borrow-returning builders + send/recv
/// reads dispatch through BOTH engine shapes with NO panic. On the Pipelined
/// arm these previously `panic!`'d ("not available on a pipelined server");
/// now the coord-resident builders resolve via coord fast paths and the
/// send-resident reads via the parked-slot lock. Driven on a fresh
/// (never-started) server, where the handles rest in their slots — the valid
/// read window. (User-keyed builders share the identical with_pipeline dispatch
/// but need a connected client; the connected-client pipeline is covered by the
/// g9pre moat + the adapter sim_integration suites.)
fn drive_borrow_builders(addr: &str, server: WorldServer<DemoEntity>) {
    let mut server = listening(addr, server);
    let mut world = DemoWorld::default();

    // Send-resident `&self` read via the parked slot — no panic.
    assert!(server.is_listening(), "server must report listening after listen()");
    assert!(
        server.scope_checks_pending().is_empty(),
        "no scope checks pending on a fresh server",
    );

    // create_room → RoomMut (coord-resident). Chain entity membership.
    let entity = world.proxy_mut().spawn_entity();
    server
        .entity_mut(world.proxy_mut(), &entity)
        .enable_replication()
        .configure_replication(ReplicationConfig::public());

    let room_key = server.create_room().key();
    assert!(server.room_exists(&room_key));
    server.room_mut(&room_key).add_entity(&entity);

    // RoomRef / RoomMut reads dispatch through the same builder.
    {
        let room = server.room(&room_key);
        assert_eq!(room.entities_count(), 1, "room must contain the added entity");
        assert!(room.has_entity(&entity), "room.has_entity must see the added entity");
        assert_eq!(room.entities(), vec![entity], "room.entities must list it");
        assert_eq!(room.users_count(), 0, "no users in the room yet");
    }

    // Global (sender-wide) priority builder — coord-resident; works in both modes.
    server.global_entity_priority_mut(entity).set_gain(5.0).boost_once(10.0);
    let pr = server.global_entity_priority(entity);
    assert_eq!(pr.gain(), Some(5.0), "global priority gain persists");
    assert_eq!(pr.accumulated(), 10.0, "global priority boost accumulates");

    // Remove the entity from the room via RoomMut (coord-resident path).
    server.room_mut(&room_key).remove_entity(&entity);
    assert_eq!(server.room(&room_key).entities_count(), 0);
}

#[test]
fn world_server_enum_resident_borrow_builders() {
    drive_borrow_builders(
        "127.0.0.1:54422",
        WorldServer::new(ServerConfig::default(), protocol()),
    );
}

#[test]
fn world_server_enum_pipelined_borrow_builders() {
    drive_borrow_builders(
        "127.0.0.1:54423",
        WorldServer::new_pipelined(ServerConfig::default(), protocol()),
    );
}
