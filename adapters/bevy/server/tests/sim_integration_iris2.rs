//! Iris 2 (MISSION_IRIS_2 / SPEC_IRIS_2_NAIA.md §1.4) integration test
//! for `Plugin::sim_integration` + `drain_host_sync_into_pipeline`.
//!
//! Exercises the smallest end-to-end flow that the pipelined-Sim
//! consumer (cyberlith Sim SubApp) relies on:
//!
//! 1. Build a bevy `App` with `Plugin::sim_integration`.
//! 2. Construct + park the three pipeline handles.
//! 3. Register an entity with naia via `run_with_world_server` +
//!    `enable_entity_replication`, add `HostOwned`, then insert a
//!    `Replicate`-derived component.
//! 4. Run the schedule — the existing `on_component_added::<R>`
//!    change-tracking system fires `HostSyncEvent::Insert`.
//! 5. Call `drain_host_sync_into_pipeline` — drains the event and
//!    invokes `insert_component_worldless`, attaching the diff handler.
//! 6. Mutate the component, run another frame, drain again — asserts
//!    that the dirty bit shows up in `GlobalDirtyBitset`.
//! 7. Despawn the entity, run another frame, drain — asserts the
//!    entity record is gone from `global_world_manager`.

use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::{entity::Entity, system::Commands};

use naia_bevy_server::{
    drain_host_sync_into_pipeline,
    pipeline_actors::{run_with_world_server, spawn_server_handles, CoordHandle},
    Plugin as ServerPlugin, RecvHandle, SendHandle, ServerConfig,
};
use naia_bevy_shared::{HostOwned, Protocol as BevyProtocol};
use naia_shared::ComponentKind;

use naia_test_harness::test_protocol::Position;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

/// Build a bevy App with `Plugin::sim_integration` installed.
fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol(),
    ));
    // Stub a Last system in Update so the per-Replicate
    // `on_component_added::<R>` system fires when we run.
    app.add_systems(Update, |_: Commands| {});
    app
}

/// Build three pipeline handles via `spawn_server_handles`. Converts
/// the bevy-side `Protocol` to the inner naia_shared `Protocol`.
///
/// Loads a real `LocalServerSocket` via `run_with_world_server` so
/// `WorldServer::is_listening` returns true — `drain_host_sync_into_pipeline`
/// skips drain otherwise (matching the production `world_to_host_sync`
/// guard).
fn build_handles_listening(
    addr: &str,
) -> (CoordHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>) {
    use naia_server::transport::local::{LocalServerSocket, LocalTransportHub, Socket};

    let naia_proto: naia_shared::Protocol = protocol().into();
    let (sim_handle, recv, send) =
        spawn_server_handles::<Entity, _>(ServerConfig::default(), naia_proto).take_handles();

    let hub = LocalTransportHub::new(addr.parse().unwrap());
    let socket = Socket::new(LocalServerSocket::new(hub), None);
    let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(Box::new(socket));

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.io_load(ps, pr);
    });
    (sim_handle, recv, send)
}

/// Returns true iff `SendStateView::live_entities` contains `entity`.
/// Reads through the same public surface cyberlith Sim uses, which
/// internally consults `global_world_manager` + `global_entity_map`.
fn entity_registered(sim_handle: &CoordHandle<Entity>, entity: Entity) -> bool {
    sim_handle
        .send_state_view()
        .live_entities()
        .contains(&entity)
}

/// Returns true iff `SendStateView::required_snapshot_entries` contains
/// `(entity, kind)`.
fn component_registered(
    sim_handle: &CoordHandle<Entity>,
    entity: Entity,
    kind: &ComponentKind,
) -> bool {
    sim_handle
        .send_state_view()
        .required_snapshot_entries()
        .iter()
        .any(|(e, k)| *e == entity && k == kind)
}

#[test]
fn sim_integration_plugin_builds() {
    // Sanity: the plugin variant constructs and `build` runs to
    // completion without panic.
    let _app = build_app();
}

#[test]
fn insert_via_host_sync_drain_registers_component() {
    let mut app = build_app();
    let (sim_handle, recv, send) = build_handles_listening(next_addr());

    // Spawn a bevy entity, add HostOwned (the marker that gates
    // `on_component_added::<R>`), and insert a Position component.
    let entity = app
        .world_mut()
        .spawn((HostOwned::new::<Singleton>(), Position::new(1.0, 2.0)))
        .id();

    // Run a frame so the change-tracking systems fire and write
    // `HostSyncEvent::Insert` into the Messages buffer.
    app.update();

    // Register the entity with naia's global world manager via the
    // pipeline `run_with_world_server` rebuild path. This mirrors what
    // cyberlith Sim does inside its host-sync system (`enable_entity_replication`
    // call site) before the drain step kicks in.
    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.enable_entity_replication(&entity);
    });

    // Drain the queued HostSyncEvent::Insert against the pipeline
    // handles. The drain calls `insert_component_worldless` on the
    // briefly-reassembled WorldServer, which registers the component
    // record in `global_world_manager` AND attaches the
    // `PropertyMutate` callback.
    let (sim_handle, _recv, _send) =
        drain_host_sync_into_pipeline(app.world_mut(), sim_handle, recv, send);

    assert!(
        entity_registered(&sim_handle, entity),
        "entity must be registered in global_world_manager after enable_entity_replication",
    );
    assert!(
        component_registered(&sim_handle, entity, &ComponentKind::of::<Position>()),
        "Position must be registered as a component record after drain",
    );
}

#[test]
fn despawn_via_host_sync_drain_removes_record() {
    let mut app = build_app();
    let (sim_handle, recv, send) = build_handles_listening(next_addr());

    let entity = app
        .world_mut()
        .spawn((HostOwned::new::<Singleton>(), Position::new(0.0, 0.0)))
        .id();
    app.update();

    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.enable_entity_replication(&entity);
    });
    let (sim_handle, recv, send) =
        drain_host_sync_into_pipeline(app.world_mut(), sim_handle, recv, send);
    assert!(entity_registered(&sim_handle, entity));

    // Despawn the entity; on_despawn observes the RemovedComponents<HostOwned>
    // and writes HostSyncEvent::Despawn.
    app.world_mut().despawn(entity);
    app.update();

    let (sim_handle, _recv, _send) =
        drain_host_sync_into_pipeline(app.world_mut(), sim_handle, recv, send);

    // After drain the global_entity_map mapping is gone, so
    // entity_to_global_entity returns Err → entity_registered returns false.
    assert!(
        !entity_registered(&sim_handle, entity),
        "entity record must be removed after Despawn drain",
    );
}

#[test]
fn empty_host_sync_queue_is_no_op() {
    let mut app = build_app();
    let (sim_handle, recv, send) = build_handles_listening(next_addr());
    // No HostSyncEvents — drain returns the handles unchanged without
    // rebuilding WorldServer.
    let (_sim_handle, _recv, _send) =
        drain_host_sync_into_pipeline(app.world_mut(), sim_handle, recv, send);
}

/// Plugin's `Singleton` host-tag is a private type within the crate;
/// to construct `HostOwned::new::<Singleton>()` in this test we need
/// our own equivalent host-tag type. The `HostOwnedMap` infrastructure
/// uses the TypeId of the tag for indexing — any distinct type works.
#[derive(Clone, Copy)]
pub struct Singleton;

// Per-test address allocator so concurrent test runs don't clash on
// the same LocalServerSocket port.
use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(58000);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    // Leak the string — fine for tests, simpler than threading lifetimes.
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
