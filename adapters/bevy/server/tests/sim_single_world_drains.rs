//! MISSION_SINGLE_WORLD_CELL — coverage for the single-world pipeline drains
//! `Server::pipeline_drain_resource_registrations_single`,
//! `Server::pipeline_drain_host_sync_single`, and
//! `Server::pipeline_drain_scope_authority_ops_single`.
//!
//! In the single-world collapse the coordinator (`ServerImpl::WorldOnly`
//! resource) and the host-owned gameplay entities live in ONE `World`. The
//! two-world `pipeline_drain_*` helpers take two distinct `&mut World` (main +
//! sim) and cannot serve a single world: lifting the coord borrow and reading
//! the entity proxy from the SAME world would alias `&mut World`. The
//! `_single` variants resolve this by draining the work-queue / reading the
//! pending events BEFORE lifting the `ServerImpl` resource, then using the
//! de-resourced world handed back by `world_only_resource_scope` as the proxy
//! source.
//!
//! The registration/host-sync *correctness* is byte-identical to the two-world
//! helpers (proven by `sim_replicated_resources.rs` / `sim_host_sync.rs`); the
//! UNIQUE risk these variants introduce is the single-world borrow discipline —
//! does it alias-panic? These tests run both drains against the REAL
//! `Plugin::pipelined` server inside a park window and assert they complete
//! without panic while preserving the carrier/entity state.

use std::{thread, time::Duration};

use bevy_app::{App, Update};
use bevy_ecs::system::Commands;

use naia_bevy_server::{
    transport, PendingScopeAuthorityOps, PipelineConfig, PipelineScopeAuthorityOp,
    Plugin as ServerPlugin, ReplicationConfig, Server, ServerConfig,
};
use naia_bevy_shared::{HostOwned, Protocol as BevyProtocol};
use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

use naia_test_harness::test_protocol::{Position, TestScore};

/// Host-tag for `HostOwned` (the plugin's own `Singleton` is crate-private).
/// `HostOwnedMap` indexes by the tag's TypeId — any distinct type works.
#[derive(Clone, Copy)]
struct Singleton;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.add_resource::<TestScore>();
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
    // Stub system so the per-Replicate change-tracking systems fire.
    app.add_systems(Update, |_: Commands| {});
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
fn resource_registration_single_drains_in_park_without_alias_panic() {
    let mut app = build_app();
    listen_and_start(&mut app, next_addr());
    app.update();
    thread::sleep(Duration::from_millis(10));

    // Replicate a resource: inserts Res<TestScore> + tags the hidden carrier
    // HostOwned + enqueues onto PendingResourceRegistrations.
    Server::pipeline_replicate_resource(app.world_mut(), TestScore::new(7, 3));
    assert_eq!(
        app.world().get_resource::<TestScore>().map(|r| *r.home),
        Some(7),
        "pipeline_replicate_resource must populate Res<TestScore> (aliasing)"
    );
    app.update();

    // Create a room (coordinator op) then drain the pending registration — both
    // through the unified single world, inside the park window.
    Server::pipeline_park(app.world());
    let room_key =
        Server::world_only_resource_scope(app.world_mut(), |_world, ws| ws.create_room().key());
    let config = ReplicationConfig::public().persist_on_scope_exit();
    Server::pipeline_drain_resource_registrations_single(app.world_mut(), &room_key, config);
    // A second drain proves the outbox emptied (idempotent no-op, no panic).
    Server::pipeline_drain_resource_registrations_single(app.world_mut(), &room_key, config);
    Server::pipeline_unpark(app.world());

    // The carrier survived registration (not despawned/clobbered) — Res<R> intact.
    assert_eq!(
        app.world().get_resource::<TestScore>().map(|r| *r.home),
        Some(7),
        "the registered carrier must survive the single-world drain (Res<R> intact)"
    );
    Server::pipeline_propagate_panics(app.world());
    drop(app);
}

#[test]
fn host_sync_single_drains_in_park_without_alias_panic() {
    let mut app = build_app();
    listen_and_start(&mut app, next_addr());
    app.update();
    thread::sleep(Duration::from_millis(10));

    // Spawn a host-owned entity; the frame fires HostSyncEvent::Insert onto the
    // pending host-sync queue.
    let entity = app
        .world_mut()
        .spawn((Position::new(1.0, 2.0), HostOwned::new::<Singleton>()))
        .id();
    app.update();

    // Drain the host-sync events through the unified single world, inside park.
    // The entity must be replication-enabled first (the host-sync drain inserts
    // its component into send_state, which requires a registered entity) —
    // mirrors `sim_host_sync.rs`'s `enable_entity_replication` before drain.
    Server::pipeline_park(app.world());
    Server::world_only_resource_scope(app.world_mut(), |_world, ws| {
        ws.enable_entity_replication(&entity);
    });
    Server::pipeline_drain_host_sync_single(app.world_mut());
    // Second drain proves the queue emptied (clean no-op, no panic).
    Server::pipeline_drain_host_sync_single(app.world_mut());
    Server::pipeline_unpark(app.world());

    assert!(
        app.world().get_entity(entity).is_ok(),
        "the host-owned entity must survive the single-world host-sync drain"
    );
    Server::pipeline_propagate_panics(app.world());
    drop(app);
}

#[test]
fn scope_authority_ops_single_drains_in_park_without_alias_panic() {
    let mut app = build_app();
    listen_and_start(&mut app, next_addr());
    app.update();
    thread::sleep(Duration::from_millis(10));

    // Spawn + replication-enable a host-owned entity (RoomAdd/authority ops
    // require a registered entity, mirroring the host-sync drain).
    let entity = app
        .world_mut()
        .spawn((Position::new(3.0, 4.0), HostOwned::new::<Singleton>()))
        .id();
    app.update();

    Server::pipeline_park(app.world());
    let room_key =
        Server::world_only_resource_scope(app.world_mut(), |_world, ws| {
            ws.enable_entity_replication(&entity);
            ws.create_room().key()
        });

    // Enqueue a RoomAdd + a TakeAuthority for the same entity onto the single
    // world's pending queue (the two-phase drain must normalize the RoomAdd
    // ahead of the authority op).
    app.world_mut().insert_resource(PendingScopeAuthorityOps(vec![
        PipelineScopeAuthorityOp::TakeAuthority(entity),
        PipelineScopeAuthorityOp::RoomAdd(entity),
    ]));

    Server::pipeline_drain_scope_authority_ops_single(app.world_mut(), &room_key);
    // Second drain proves the queue emptied (clean no-op, no panic).
    Server::pipeline_drain_scope_authority_ops_single(app.world_mut(), &room_key);
    Server::pipeline_unpark(app.world());

    assert!(
        app.world().get_entity(entity).is_ok(),
        "the entity must survive the single-world scope/authority drain"
    );
    Server::pipeline_propagate_panics(app.world());
    drop(app);
}

use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(23040);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
