use naia_test_harness::{EntityKey, Position, Velocity};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";
const SPAWN_POSITION_VALUE_KEY: &str = "spawn_position_value";
const SPAWN_VELOCITY_VALUE_KEY: &str = "spawn_velocity_value";

// ============================================================================
// Contract 18 — SpawnWithComponents Coalesce (Phase 4)
// ============================================================================

/// Spawns a server-owned entity with both Position and Velocity components.
/// Used to verify multi-component coalesced spawn behavior.
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

/// Spawns a server-owned entity with no replicated components.
/// Used to verify the legacy zero-component Spawn path is preserved.
#[given("a server-owned entity exists without any replicated components")]
fn given_entity_without_any_replicated_components(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| server.spawn(|_| {}));
        entity_key
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Asserts the entity is visible on the client with both Position and Velocity present.
#[then("the entity spawns on the client with Position and Velocity")]
fn then_entity_spawns_with_position_and_velocity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() && entity.has_component::<Velocity>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Asserts entity is visible on client with correct initial Position + Velocity values.
#[then("the entity spawns on the client with correct Position and Velocity values")]
fn then_entity_spawns_with_correct_values(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected_pos: (f32, f32) = ctx
        .scenario()
        .bdd_get(SPAWN_POSITION_VALUE_KEY)
        .expect("No position value stored");
    let expected_vel: (f32, f32) = ctx
        .scenario()
        .bdd_get(SPAWN_VELOCITY_VALUE_KEY)
        .expect("No velocity value stored");

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        let Some(vel) = entity.component::<Velocity>() else {
            return AssertOutcome::Pending;
        };

        let pos_ok = (*pos.x - expected_pos.0).abs() < f32::EPSILON
            && (*pos.y - expected_pos.1).abs() < f32::EPSILON;
        let vel_ok = (*vel.vx - expected_vel.0).abs() < f32::EPSILON
            && (*vel.vy - expected_vel.1).abs() < f32::EPSILON;

        if pos_ok && vel_ok {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

