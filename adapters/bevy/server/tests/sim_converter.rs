//! MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19) —
//! `SimConverter<Entity>` resource installable on a Sim Bevy app.
//!
//! Verifies the new converter facade:
//! - A `SimConverter` built from a `SimHandle` (via
//!   `SimConverter::from_sim`) installs on a Bevy `App` as a
//!   `Resource`.
//! - For every entity registered via the legacy `WorldServer` path,
//!   the `SimConverter` returns the SAME `GlobalEntity` as
//!   `WorldServer`'s built-in `EntityAndGlobalEntityConverter` impl —
//!   the byte-identity precondition for `EntityProperty` wire output.
//! - `EntityProperty::set(&sim_converter, &entity).get_inner()`
//!   returns the same `Option<GlobalEntity>` as
//!   `EntityProperty::set(&world_server, &entity).get_inner()`. Since
//!   `EntityProperty`'s entire wire payload is a function of this
//!   `GlobalEntity`, identical inner state ⇒ byte-identical wire output.

use std::time::Duration;

use bevy_app::App;
use bevy_ecs::entity::Entity;

use naia_bevy_server::{
    pipeline_actors::{run_with_world_server, spawn_server_handles},
    Plugin as ServerPlugin, ServerConfig, SimConverter,
};
use naia_bevy_shared::{EntityAndGlobalEntityConverter, EntityProperty, Protocol as BevyProtocol};
use naia_test_harness::test_protocol::Position;

fn protocol() -> naia_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    let bevy_proto = p.build();
    bevy_proto.into()
}

fn handles() -> (
    naia_server::pipeline_actors::SimHandle<Entity>,
    naia_server::RecvHandle<Entity>,
    naia_server::SendHandle<Entity>,
) {
    spawn_server_handles::<Entity, _>(ServerConfig::default(), protocol())
}

#[test]
fn sim_converter_installs_as_sim_resource() {
    let (sim_handle, _recv, _send) = handles();
    let sim_converter = SimConverter::from_sim(&sim_handle);

    let mut sim_app = App::new();
    sim_app.add_plugins(ServerPlugin::sim_integration(
        ServerConfig::default(),
        protocol_bevy(),
    ));
    sim_app.insert_resource(sim_converter);

    assert!(
        sim_app.world().get_resource::<SimConverter>().is_some(),
        "SimConverter must install as a Sim Bevy Resource",
    );
}

#[test]
fn sim_converter_global_entity_matches_world_server_for_registered_entities() {
    let (sim_handle, recv, send) = handles();
    let sim_converter = SimConverter::from_sim(&sim_handle);

    // Register several entities through the legacy WorldServer path
    // so they exist in `shared.global_entity_map`. Use the
    // run_with_world_server reassembly to drive
    // `enable_entity_replication` (the same path cyberlith uses pre-Phase-C).
    let mut scratch_world = bevy_ecs::world::World::new();
    let dummy_entities: Vec<Entity> = (0..4).map(|_| scratch_world.spawn(()).id()).collect();
    let (sim_handle, _recv, _send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        for e in &dummy_entities {
            ws.enable_entity_replication(e);
        }
    });

    // For every registered entity, the SimConverter and the
    // run_with_world_server converter view must return identical
    // GlobalEntity values.
    let (sim_handle, _recv, _send, ()) = run_with_world_server(sim_handle, _recv, _send, |ws| {
        for e in &dummy_entities {
            let from_ws = ws.entity_to_global_entity(e).expect("ws lookup");
            let from_sim = sim_converter
                .entity_to_global_entity(e)
                .expect("sim lookup");
            assert_eq!(
                from_ws, from_sim,
                "SimConverter must return the same GlobalEntity as WorldServer for entity {:?}",
                e,
            );
            // Round-trip
            let back_ws = ws.global_entity_to_entity(&from_ws).expect("ws back");
            let back_sim = sim_converter
                .global_entity_to_entity(&from_sim)
                .expect("sim back");
            assert_eq!(back_ws, back_sim);
            assert_eq!(back_ws, *e);
        }
    });
    let _ = sim_handle; // silence unused
}

#[test]
fn entity_property_set_byte_identical_between_sim_converter_and_world_server() {
    let (sim_handle, recv, send) = handles();
    let sim_converter = SimConverter::from_sim(&sim_handle);

    // Register one entity.
    let mut scratch_world = bevy_ecs::world::World::new();
    let entity = scratch_world.spawn(()).id();
    let (sim_handle, recv, send, ()) = run_with_world_server(sim_handle, recv, send, |ws| {
        ws.enable_entity_replication(&entity);
    });

    // Build EntityProperty via SimConverter.
    let mut prop_sim = EntityProperty::new_for_message();
    prop_sim.set(&sim_converter, &entity);

    // Build EntityProperty via WorldServer.
    let (_sim_handle, _recv, _send, prop_ws_inner) =
        run_with_world_server(sim_handle, recv, send, |ws| {
            let mut prop_ws = EntityProperty::new_for_message();
            prop_ws.set(ws, &entity);
            prop_ws.get_inner()
        });

    // Inner GlobalEntity is the wire-payload generator for
    // EntityProperty. Identical inner state ⇒ identical wire bytes.
    assert!(prop_sim.get_inner().is_some(), "set must populate inner");
    assert_eq!(
        prop_sim.get_inner(),
        prop_ws_inner,
        "EntityProperty inner GlobalEntity must match between SimConverter and WorldServer paths",
    );
}

#[test]
fn sim_converter_unknown_entity_returns_error() {
    let (sim_handle, _recv, _send) = handles();
    let sim_converter = SimConverter::from_sim(&sim_handle);

    let mut scratch_world = bevy_ecs::world::World::new();
    let unknown = scratch_world.spawn(()).id();
    assert!(
        sim_converter.entity_to_global_entity(&unknown).is_err(),
        "unknown entity must error (no GlobalEntity registered)",
    );
}

fn protocol_bevy() -> naia_bevy_shared::Protocol {
    let mut p = BevyProtocol::builder();
    p.register_component::<Position>();
    p.tick_interval(Duration::from_micros(100));
    p.build()
}
