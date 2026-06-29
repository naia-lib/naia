//! Sim-integration-mode Replicated Resources via bevy-0.19 storage aliasing.
//!
//! Under bevy 0.19 a `ReplicatedResource` is both a bevy `Component` and a
//! bevy `Resource` sharing one storage cell, so the carrier entity-component
//! IS `Res<R>`. In sim-integration mode there is therefore NO
//! resource-specific naia code: a replicated resource is just a **host-owned
//! component-entity in the sim world** that rides the existing
//! `HostSyncEvent` pipeline like any other entity. This test proves:
//!
//! 1. Spawning the host-owned carrier populates `Res<R>` (aliasing).
//! 2. The carrier registers via the normal host-sync drain (no resource code),
//!    which attaches the native naia `PropertyMutator`.
//! 3. `get_resource_mut::<R>()` mutates the very component that is registered
//!    for replication (aliasing) — the carrier component reflects the write.
//!    End-to-end per-field diff via the native naia `PropertyMutator` (no
//!    mirror) is proven by the standard-mode `f5_echo_prevention_*` test,
//!    which goes server-`ResMut<R>` → wire → client with NO mirror; sim mode
//!    uses the identical native mutator (attached by the same
//!    `insert_component_worldless` during the host-sync drain).
//! 4. "Removing" the resource via the despawn chokepoint
//!    (`WorldMut::despawn_entity`) does NOT panic and clears `Res<R>`.

use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::{entity::Entity, system::Commands};

use naia_bevy_server::{
    drain_host_sync_into_pipeline,
    pipeline_actors::{run_with_world_server, spawn_server_handles, CoordHandle},
    Plugin as ServerPlugin, RecvHandle, SendHandle, ServerConfig,
};
use naia_bevy_shared::{HostOwned, Protocol as BevyProtocol, WorldMutType, WorldProxyMut};
use naia_shared::ComponentKind;

use naia_test_harness::test_protocol::TestScore;

/// Host-tag for `HostOwned` (the plugin's `Singleton` is crate-private).
/// `HostOwnedMap` indexes by the tag's TypeId — any distinct type works.
#[derive(Clone, Copy)]
pub struct Singleton;

fn protocol() -> BevyProtocol {
    let mut p = BevyProtocol::builder();
    p.add_resource::<TestScore>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol(),
    ));
    // Stub system so the per-Replicate change-tracking systems fire.
    app.add_systems(Update, |_: Commands| {});
    app
}

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

fn res_home(app: &App) -> Option<u32> {
    app.world().get_resource::<TestScore>().map(|r| *r.home)
}

#[test]
fn sim_resource_round_trip_update_and_remove() {
    let mut app = build_app();
    let (sim_handle, recv, send) = build_handles_listening(next_addr());

    // (1) A replicated resource in sim mode is just a host-owned carrier
    // entity. Spawning it populates Res<TestScore> via aliasing.
    let entity = app
        .world_mut()
        .spawn((TestScore::new(7, 3), HostOwned::new::<Singleton>()))
        .id();
    assert_eq!(
        res_home(&app),
        Some(7),
        "spawning the host-owned carrier must populate Res<TestScore> (aliasing)"
    );

    // Run a frame so on_component_added fires HostSyncEvent::Insert.
    app.update();

    // (2) Register the carrier with naia + drain the Insert — this attaches
    // the NATIVE naia PropertyMutator to the carrier component (== Res<R>).
    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.enable_entity_replication(&entity);
    });
    let (sim_handle, recv, send) =
        drain_host_sync_into_pipeline(app.world_mut(), sim_handle, recv, send);
    assert!(
        component_registered(&sim_handle, entity, &ComponentKind::of::<TestScore>()),
        "carrier TestScore must be registered as a component record after drain",
    );

    // (3) Update via get_resource_mut (== mutating the registered carrier
    // component via aliasing). The write must land on the SAME component that
    // replicates — assert both the resource view and the entity-component
    // reflect it.
    let cell = parking_lot::Mutex::new(Some(()));
    let id = app
        .world_mut()
        .register_system(move |mut score: bevy_ecs::system::ResMut<TestScore>| {
            if cell.lock().take().is_some() {
                *score.home = 99;
            }
        });
    app.world_mut().run_system(id).expect("mutate via ResMut");
    app.update();

    // Aliasing: Res<R> reflects the write (it IS the carrier component).
    assert_eq!(
        res_home(&app),
        Some(99),
        "get_resource_mut write must be visible (aliases the carrier component)"
    );
    // ...and the registered entity-component is the same storage.
    {
        let world = app.world_mut();
        let mut proxy = world.proxy_mut();
        let comp = proxy
            .component_mut::<TestScore>(&entity)
            .expect("carrier entity-component present");
        assert_eq!(
            *comp.home, 99,
            "the registered carrier component must reflect the get_resource_mut write \
             (it is the same storage that replicates via the native mutator)"
        );
    }

    // (4) Remove the resource via the despawn chokepoint. This must NOT panic
    // (the carrier aliases a Resource cell — World::despawn would panic) and
    // must clear Res<TestScore>.
    {
        let world = app.world_mut();
        let mut proxy = world.proxy_mut();
        WorldMutType::<Entity>::despawn_entity(&mut proxy, &entity);
    }
    assert_eq!(
        res_home(&app),
        None,
        "removing the carrier (chokepoint -> component-remove) must clear Res<TestScore>"
    );

    let _ = (recv, send, sim_handle);
}

use std::sync::atomic::{AtomicUsize, Ordering};
static PORT_COUNTER: AtomicUsize = AtomicUsize::new(58500);
fn next_addr() -> &'static str {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    Box::leak(format!("127.0.0.1:{port}").into_boxed_str())
}
