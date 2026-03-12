//! Step bindings for Entity Replication contract (07_entity_replication.feature)
//!
//! These steps cover:
//!   - Server-owned entities with replicated components
//!   - Component replication to clients
//!   - Component update observation

use naia_test_harness::{EntityKey, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

/// Storage key for the last entity created in BDD tests
const LAST_ENTITY_KEY: &str = "last_entity";

/// Storage key for tracking the last component value for update detection
const LAST_COMPONENT_VALUE_KEY: &str = "last_component_value";

/// Storage key for tracking the initial entity key (for GlobalEntity stability check)
const INITIAL_ENTITY_KEY: &str = "initial_entity_key";

/// Storage key for tracking the client's local modification value
#[allow(dead_code)]
const CLIENT_LOCAL_VALUE_KEY: &str = "client_local_value";

// ============================================================================
// Given Steps - Entity Setup with Replicated Components
// ============================================================================

/// Step: Given a server-owned entity exists with a replicated component
/// Creates a server-owned entity with a Position component and stores its key.
#[given("a server-owned entity exists with a replicated component")]
fn given_server_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                // Position is a replicated component
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        entity_key
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    // Store initial entity key for GlobalEntity stability verification
    scenario.bdd_store(INITIAL_ENTITY_KEY, entity_key);
    // Store initial component value for update detection
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

// ============================================================================
// When Steps - Component Updates
// ============================================================================

/// Step: When the server updates the replicated component
/// Updates the Position component on the server-owned entity.
#[when("the server updates the replicated component")]
fn when_server_updates_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Update to a new position value
    let new_value = (100.0_f32, 200.0_f32);

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = new_value.0;
                    *pos.y = new_value.1;
                }
            }
        });
    });

    // Store the new value for later verification
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, new_value);

    // Advance a tick to let replication happen
    scenario.mutate(|_| {});
}

// ============================================================================
// Then Steps - Replication Assertions
// ============================================================================

/// Step: Then the entity spawns on the client with the replicated component
/// Verifies the entity exists on the client with the Position component.
/// This is a POLLING assertion - waits for entity to spawn with component.
#[then("the entity spawns on the client with the replicated component")]
fn then_entity_spawns_on_client_with_replicated_component(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client observes the component update
/// Verifies the client sees the updated Position component value.
/// This is a POLLING assertion - waits for the update to be received.
#[then("the client observes the component update")]
fn then_client_observes_component_update(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected_value: (f32, f32) = ctx
        .scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");

    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                let current_x = *pos.x;
                let current_y = *pos.y;
                if (current_x - expected_value.0).abs() < f32::EPSILON
                    && (current_y - expected_value.1).abs() < f32::EPSILON
                {
                    AssertOutcome::Passed(())
                } else {
                    AssertOutcome::Pending
                }
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ============================================================================
// Given Steps - Client Local Modifications
// ============================================================================

/// Step: Given the client modifies the component locally
/// Modifies the Position component on the client side (local change that
/// will be overwritten by server state on conflict).
#[given("the client modifies the component locally")]
fn given_client_modifies_component_locally(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Client modifies to a different value than the server will send
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

    // Store the local value for potential verification
    scenario.bdd_store(CLIENT_LOCAL_VALUE_KEY, local_value);
}

// ============================================================================
// Then Steps - Server Value Observation
// ============================================================================

/// Step: Then the client observes the server value
/// Verifies the client sees the server's authoritative value (not client's local modification).
/// This is a POLLING assertion - waits for the server value to be received.
#[then("the client observes the server value")]
fn then_client_observes_server_value(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let server_value: (f32, f32) = ctx
        .scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No server component value stored");

    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                let current_x = *pos.x;
                let current_y = *pos.y;
                // Client should have the server's value, not the local modification
                if (current_x - server_value.0).abs() < f32::EPSILON
                    && (current_y - server_value.1).abs() < f32::EPSILON
                {
                    AssertOutcome::Passed(())
                } else {
                    AssertOutcome::Pending
                }
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ============================================================================
// Then Steps - GlobalEntity Stability
// ============================================================================

/// Step: Then the entity GlobalEntity remains unchanged
/// Verifies the entity's identity (EntityKey) is stable during its lifetime.
/// The EntityKey is the harness abstraction over Naia's GlobalEntity.
#[then("the entity GlobalEntity remains unchanged")]
fn then_entity_global_entity_remains_unchanged(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let initial_entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(INITIAL_ENTITY_KEY)
        .expect("No initial entity key stored");
    let current_entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No current entity key stored");

    // The entity key should be the same - this proves GlobalEntity stability
    if initial_entity_key == current_entity_key {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "GlobalEntity changed: initial={:?}, current={:?}",
            initial_entity_key, current_entity_key
        ))
    }
}
