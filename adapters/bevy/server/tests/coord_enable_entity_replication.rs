//! MISSION_USER_ONLY_SEES_SIM Phase D.2.1 (2026-05-19) —
//! `CoordHandle::enable_entity_replication` Coord-only entry point.
//!
//! Audit of `WorldServer::enable_entity_replication`
//! (`world_server.rs:1044`) -> `spawn_entity_inner`
//! (`world_server.rs:1025`) shows the call exclusively writes Coord-side
//! shared state (`global_entity_map`, `global_world_manager`,
//! `idx_to_world`). No Send-side mutation, no world-hook registration.
//! Exposing as a `CoordHandle` method is therefore the entire delta —
//! no `ScopeChange::EnableReplication` variant needed (nothing to
//! defer).
//!
//! Coverage:
//! - byte-identical shared-state observables (`entity_replication_config`,
//!   `entity_owner`) vs the legacy `WorldServer::enable_entity_replication`
//!   path,
//! - ordering preservation across an enable+configure sequence in the
//!   same tick (the new Coord-only enable must compose with the
//!   existing B.2 `configure_entity_replication` facade without
//!   altering wire effect).

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{
        configure_entity_replication, run_with_world_server, spawn_server_handles,
    },
    EntityOwner, Plugin as ServerPlugin, ReplicationConfig, ScopeExit, ServerConfig,
};
use naia_bevy_shared::{Protocol as BevyProtocol, WorldProxyMut};
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn protocol_bevy() -> naia_bevy_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.add_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}

fn handles() -> (
    naia_server::pipeline_actors::CoordHandle<Entity>,
    naia_server::RecvHandle<Entity>,
    naia_server::SendHandle<Entity>,
) {
    spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol())
}

#[test]
fn coord_enable_matches_legacy_observables() {
    // Parity: register via CoordHandle::enable_entity_replication on
    // one server, via WorldServer::enable_entity_replication under
    // reassembly on another. The observable shared-state reads
    // (entity_replication_config + entity_owner) must agree
    // byte-for-byte.

    fn run_with(coord_path: bool) -> (ReplicationConfig, EntityOwner) {
        let (mut coord, recv, send) = handles();
        let mut sim_app = App::new();
        sim_app.add_plugins(ServerPlugin::sim_integration(
            ServerConfig::default(),
            protocol_bevy(),
        ));
        let entity = sim_app.world_mut().spawn(()).id();

        let (coord, recv, send) = if coord_path {
            coord.enable_entity_replication(&entity);
            (coord, recv, send)
        } else {
            let (coord, recv, send, ()) = run_with_world_server(coord, recv, send, |ws| {
                ws.enable_entity_replication(&entity);
            });
            (coord, recv, send)
        };

        let owner = coord.entity_owner(&entity);
        let (_coord, _recv, _send, cfg) =
            run_with_world_server(coord, recv, send, |ws| {
                ws.entity_replication_config(&entity)
                    .expect("entity registered")
            });
        (cfg, owner)
    }

    let from_coord = run_with(true);
    let from_legacy = run_with(false);
    assert_eq!(
        from_coord, from_legacy,
        "Coord-only enable_entity_replication must produce byte-identical \
         shared-state observables to the legacy WorldServer path",
    );
    // Sanity: matches the documented defaults for server-spawned entities.
    assert_eq!(from_coord.0, ReplicationConfig::public());
    assert_eq!(from_coord.0.scope_exit, ScopeExit::Despawn);
    assert!(matches!(from_coord.1, EntityOwner::Server));
}

#[test]
fn coord_enable_composes_with_configure_in_same_tick() {
    // Ordering preservation: a Sim system enables + configures the same
    // entity within one tick using the new Coord-only enable + the
    // existing B.2 configure facade. The resulting replication_config
    // must match the legacy `enable + configure` sequence under
    // reassembly. This is the cyberlith `drain_sim_tile_registrations`
    // shape (per `server_access.rs:1127`): each tile gets both ops
    // back-to-back.

    fn run_with(coord_enable: bool) -> ReplicationConfig {
        let (mut coord, recv, send) = handles();
        let mut sim_app = App::new();
        sim_app.add_plugins(ServerPlugin::sim_integration(
            ServerConfig::default(),
            protocol_bevy(),
        ));
        let entity = sim_app.world_mut().spawn(()).id();

        let (coord, recv, send) = if coord_enable {
            coord.enable_entity_replication(&entity);
            (coord, recv, send)
        } else {
            let (coord, recv, send, ()) = run_with_world_server(coord, recv, send, |ws| {
                ws.enable_entity_replication(&entity);
            });
            (coord, recv, send)
        };

        let target = ReplicationConfig::public().persist_on_scope_exit();
        let (coord, recv, send) = {
            let world: &mut World = sim_app.world_mut();
            let mut proxy = WorldProxyMut::proxy_mut(world);
            configure_entity_replication(coord, recv, send, &mut proxy, &entity, target)
        };

        let (_coord, _recv, _send, cfg) = run_with_world_server(coord, recv, send, |ws| {
            ws.entity_replication_config(&entity).expect("registered")
        });
        cfg
    }

    let from_coord = run_with(true);
    let from_legacy = run_with(false);
    assert_eq!(
        from_coord, from_legacy,
        "enable (Coord) + configure (facade) must equal enable + configure \
         under legacy reassembly",
    );
    assert_eq!(from_coord.scope_exit, ScopeExit::Persist);
}
