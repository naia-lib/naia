//! MISSION_USER_ONLY_SEES_SIM Phase B.2 (2026-05-19) —
//! `pipeline_actors::configure_entity_replication` Sim-side ergonomic
//! entry point.
//!
//! Cyberlith's pre-Phase-C wrapper (`server_access::configure_entity_
//! replication_with_world`) manually inlined the
//! `WorldServer::from_pipeline_states` + `split_world_server` dance
//! around a `WorldProxyMut` borrow. Phase B.2 folds the dance into
//! one naia call so cyberlith Phase C can call this from a Sim system
//! with just take/call/put on the three Resource halves.
//!
//! Wire effect is byte-identical to the legacy `WorldServer::configure
//! _entity_replication` path by construction — `configure_entity_
//! replication` literally invokes that method under a
//! `run_with_world_server` reassembly. The value-add is the API
//! surface, not the wire body. See the function's doc-comment for
//! the design rationale (why this is a free fn that reassembles
//! rather than a `SimHandle` method that defers via a
//! `ScopeChange::ConfigureReplication` variant).
//!
//! Coverage:
//! - The new entry point produces the same `entity_replication_config`
//!   end state as a direct `WorldServer::configure_entity_replication`
//!   call (verified via `run_with_world_server` post-condition read).
//! - Identity round-trip: scope_exit Despawn → Persist → Despawn is
//!   observable through the `WorldServer::entity_replication_config`
//!   getter at each step.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::{entity::Entity, world::World};

use naia_bevy_server::{
    pipeline_actors::{configure_entity_replication, run_with_world_server, spawn_server_handles},
    Plugin as ServerPlugin, ReplicationConfig, ScopeExit, ServerConfig,
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
    naia_server::pipeline_actors::SimHandle<Entity>,
    naia_server::RecvHandle<Entity>,
    naia_server::SendHandle<Entity>,
) {
    spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol())
}

#[test]
fn configure_entity_replication_scope_exit_toggle_via_facade() {
    let (sim_handle, recv, send) = handles();

    // Build a sim-app holding the Bevy world that owns the entity. In
    // Sim-Owns-World cyberlith, this is the SimApp world.
    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    let entity = sim_app.world_mut().spawn(()).id();

    // Register the entity with naia (legacy path — Sim Phase C will
    // route through SimHandle::enable_entity_replication once that
    // API exists; today we use the reassembly).
    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.enable_entity_replication(&entity);
    });

    // Server-spawned entities default to Public + Despawn (per
    // ReplicationConfig::public()). Verify the starting state.
    let (sim_handle, recv, send, start_cfg) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.entity_replication_config(&entity)
            .expect("entity registered")
    });
    assert_eq!(start_cfg, ReplicationConfig::public());
    assert_eq!(start_cfg.scope_exit, ScopeExit::Despawn);

    // Phase B.2 facade call: flip scope_exit Despawn → Persist via the
    // new pipeline-friendly entry point (no manual reassembly).
    let target = ReplicationConfig::public().persist_on_scope_exit();
    let (sim_handle, recv, send) = {
        let world: &mut World = sim_app.world_mut();
        let mut proxy = WorldProxyMut::proxy_mut(world);
        configure_entity_replication(sim_handle, recv, send, &mut proxy, &entity, target)
    };

    // Post-condition: replication_config reflects the new scope_exit.
    let (sim_handle, recv, send, mid_cfg) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.entity_replication_config(&entity)
            .expect("entity registered")
    });
    assert_eq!(mid_cfg, target);
    assert_eq!(mid_cfg.scope_exit, ScopeExit::Persist);

    // Round-trip back to Despawn via the same facade.
    let revert = ReplicationConfig::public();
    let (sim_handle, recv, send) = {
        let world: &mut World = sim_app.world_mut();
        let mut proxy = WorldProxyMut::proxy_mut(world);
        configure_entity_replication(sim_handle, recv, send, &mut proxy, &entity, revert)
    };

    let (_sim_handle, _recv, _send, end_cfg) =
        run_with_world_server(sim_handle, recv, send, |ws| {
            ws.entity_replication_config(&entity)
                .expect("entity registered")
        });
    assert_eq!(end_cfg, revert);
    assert_eq!(end_cfg.scope_exit, ScopeExit::Despawn);
}

#[test]
fn configure_entity_replication_facade_matches_legacy_path_for_scope_exit() {
    // Parity test: run the facade on one server, the legacy
    // `WorldServer::configure_entity_replication` on another, both
    // with the same input. The resulting `entity_replication_config`
    // reads must agree.

    fn run_with(facade: bool) -> ReplicationConfig {
        let (sim_handle, recv, send) = handles();
        let mut sim_app = App::new();
        sim_app.add_plugins(ServerPlugin::sim_integration(
            ServerConfig::default(),
            protocol_bevy(),
        ));
        let entity = sim_app.world_mut().spawn(()).id();

        let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
            ws.enable_entity_replication(&entity);
        });

        let target = ReplicationConfig::public().persist_on_scope_exit();
        let (sim_handle, recv, send) = if facade {
            let world: &mut World = sim_app.world_mut();
            let mut proxy = WorldProxyMut::proxy_mut(world);
            configure_entity_replication(sim_handle, recv, send, &mut proxy, &entity, target)
        } else {
            let world: &mut World = sim_app.world_mut();
            let mut proxy = WorldProxyMut::proxy_mut(world);
            let (sim_handle, recv, send, ()) =
                run_with_world_server(sim_handle, recv, send, |ws| {
                    ws.configure_entity_replication(&mut proxy, &entity, target);
                });
            (sim_handle, recv, send)
        };

        let (_sim_handle, _recv, _send, cfg) =
            run_with_world_server(sim_handle, recv, send, |ws| {
                ws.entity_replication_config(&entity)
                    .expect("entity registered")
            });
        cfg
    }

    let from_facade = run_with(true);
    let from_legacy = run_with(false);
    assert_eq!(
        from_facade, from_legacy,
        "facade and legacy must produce byte-identical replication_config state",
    );
}
