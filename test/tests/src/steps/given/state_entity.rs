//! Given-step bindings: entity / component / replication preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;

use naia_test_harness::{ImmutableLabel, Position, Velocity};

#[given("a server-owned entity exists with only ImmutableLabel")]
fn given_entity_with_only_immutable_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(ImmutableLabel);
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with Position and ImmutableLabel.
#[given("a server-owned entity exists with Position and ImmutableLabel")]
fn given_entity_with_position_and_immutable_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .insert_component(ImmutableLabel);
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with Position and Velocity components.
#[given("a server-owned entity exists with Position and Velocity components")]
fn given_entity_with_position_and_velocity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let pos_val = (7.0_f32, 8.0_f32);
    let vel_val = (3.0_f32, 4.0_f32);
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(pos_val.0, pos_val.1));
                entity.insert_component(Velocity::new(vel_val.0, vel_val.1));
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(SPAWN_POSITION_VALUE_KEY, pos_val);
    scenario.bdd_store(SPAWN_VELOCITY_VALUE_KEY, vel_val);
}

/// Given a server-owned entity exists without any replicated components.
#[given("a server-owned entity exists without any replicated components")]
fn given_entity_without_any_replicated_components(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| server.spawn(|_| {}));
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}


// ──────────────────────────────────────────────────────────────────────
// Entity replication preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a server-owned entity exists with a replicated component.
///
/// Spawns a server-owned entity with Position(0, 0). Stores both
/// LAST_ENTITY_KEY and INITIAL_ENTITY_KEY (the latter for
/// GlobalEntity-stability tests).
#[given("a server-owned entity exists with a replicated component")]
fn given_server_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::{INITIAL_ENTITY_KEY, LAST_COMPONENT_VALUE_KEY};
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(INITIAL_ENTITY_KEY, entity_key);
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

/// Given a server-owned entity exists without a replicated component.
///
/// Spawns a bare server-owned entity. Used to test component-insert
/// events where the component is added after spawn.
#[given("a server-owned entity exists without a replicated component")]
fn given_server_owned_entity_without_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| server.spawn(|_| {}));
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(INITIAL_ENTITY_KEY, entity_key);
}

/// Given the client modifies the component locally.
///
/// Mutates Position to (999, 888) on the client side. Stores the
/// local value under `CLIENT_LOCAL_VALUE_KEY`. Used to confirm the
/// server-authoritative value overrides the client-local one.
#[given("the client modifies the component locally")]
fn given_client_modifies_component_locally(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: naia_test_harness::EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let local_value = (999.0_f32, 888.0_f32);
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = local_value.0;
                    *pos.y = local_value.1;
                }
            }
        });
    });
    scenario.bdd_store(CLIENT_LOCAL_VALUE_KEY, local_value);
}


// ──────────────────────────────────────────────────────────────────────
// Multi-entity preconditions (priority accumulator)
// ──────────────────────────────────────────────────────────────────────

/// Given two server-owned entities A and B exist each with a replicated component in-scope for the client.
///
/// Used by B-BDD-8 (per-entity convergence). Stores both keys under
/// `ENTITY_A_KEY` / `ENTITY_B_KEY` and waits for the client to
/// observe both.
#[given(
    "two server-owned entities A and B exist each with a replicated component in-scope for the client"
)]
fn given_two_entities_a_b_in_scope(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::{ENTITY_A_KEY, ENTITY_B_KEY};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();
    let mut spawn = || scenario.mutate(|c| c.server(|s|
        s.spawn(|mut e| { e.insert_component(Position::new(0.0, 0.0)); }))).0;
    let (a, b) = (spawn(), spawn());
    scenario.mutate(|c| c.server(|s| {
        for ek in [&a, &b] { if let Some(mut e) = s.entity_mut(ek) { e.enter_room(&room_key); } }
        if let Some(mut scope) = s.user_scope_mut(&client_key) {
            scope.include(&a); scope.include(&b);
        }
    }));
    scenario.bdd_store(ENTITY_A_KEY, a);
    scenario.bdd_store(ENTITY_B_KEY, b);
    scenario.expect(|ectx| ectx.client(client_key, |c|
        (c.has_entity(&a) && c.has_entity(&b)).then_some(())));
}

