//! MISSION_USER_ONLY_SEES_SIM Phase D.3b.1 (2026-05-19) —
//! `CoordHandle::mark_entity_as_static` Coord-only entry point.
//!
//! Audit of `WorldServer::mark_entity_as_static` (`world_server.rs:1133`)
//! -> `GlobalWorldManager::mark_entity_as_static`
//! (`global_world_manager.rs:104`) shows the entire op is a single
//! gwm flag write (`record.is_static = true`). No Send-side mutation, no
//! world-hook registration. Exposing as a `CoordHandle` method is
//! therefore the entire delta — no `ScopeChange` variant needed (nothing
//! to defer).
//!
//! Coverage:
//! - the new Coord-only path produces the same observable static-flag
//!   state (`entity_is_static`) as the legacy
//!   `WorldServer::mark_entity_as_static` under reassembly,
//! - the flag is observable both via the new `CoordHandle::entity_is_static`
//!   query and via the legacy `WorldServer::entity_is_static` read.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles},
    Plugin as ServerPlugin, ServerConfig,
};
use naia_bevy_shared::Protocol as BevyProtocol;
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
fn coord_mark_static_matches_legacy() {
    // Parity: enable replication, then mark static — via CoordHandle on
    // one server, via WorldServer under reassembly on another. The
    // observable static-flag state must agree.

    fn run_with(coord_path: bool) -> bool {
        let (mut coord, recv, send) = handles();
        let mut sim_app = App::new();
        sim_app.add_plugins(ServerPlugin::sim_integration(
            ServerConfig::default(),
            protocol_bevy(),
        ));
        let entity = sim_app.world_mut().spawn(()).id();

        // Register the entity (Coord-only enable is itself audited in
        // coord_enable_entity_replication.rs).
        coord.enable_entity_replication(&entity);

        if coord_path {
            coord.mark_entity_as_static(&entity);
        } else {
            let (coord, recv, send, ()) = run_with_world_server(coord, recv, send, |ws| {
                ws.mark_entity_as_static(&entity);
            });
            // Read back via the legacy WorldServer path.
            let (_coord, _recv, _send, is_static) =
                run_with_world_server(coord, recv, send, |ws| ws.entity_is_static(&entity));
            return is_static;
        }

        coord.entity_is_static(&entity)
    }

    let from_coord = run_with(true);
    let from_legacy = run_with(false);
    assert!(
        from_coord,
        "Coord-only mark_entity_as_static must set the static flag",
    );
    assert_eq!(
        from_coord, from_legacy,
        "Coord-only mark_entity_as_static must produce the same observable \
         static-flag state as the legacy WorldServer path",
    );
}

#[test]
fn coord_mark_static_observable_via_world_server() {
    // Cross-path read: set the flag via CoordHandle, read it back via the
    // legacy WorldServer::entity_is_static. They must agree (single shared
    // gwm state).

    let (mut coord, recv, send) = handles();
    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    let entity = sim_app.world_mut().spawn(()).id();

    coord.enable_entity_replication(&entity);
    assert!(
        !coord.entity_is_static(&entity),
        "freshly-enabled entity is not static",
    );

    coord.mark_entity_as_static(&entity);
    assert!(coord.entity_is_static(&entity), "Coord query reflects flag");

    let (_coord, _recv, _send, is_static) =
        run_with_world_server(coord, recv, send, |ws| ws.entity_is_static(&entity));
    assert!(
        is_static,
        "flag set via CoordHandle must be observable via WorldServer \
         (single shared gwm state)",
    );
}
