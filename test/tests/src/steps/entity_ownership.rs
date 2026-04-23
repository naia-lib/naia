//! Step bindings for Entity Ownership contract (08_entity_ownership.feature)
//!
//! These steps cover:
//!   - Ownership determination (server vs client owned)
//!   - Write rejection for unauthorized clients
//!   - Client-owned entity spawning and updates

use naia_client::ReplicationConfig as ClientReplicationConfig;
use naia_test_harness::{EntityKey, EntityOwner, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

/// Storage key for the last entity created in BDD tests
const LAST_ENTITY_KEY: &str = "last_entity";

/// Storage key for tracking the last component value for update detection
const LAST_COMPONENT_VALUE_KEY: &str = "last_component_value";

/// Storage key for tracking whether a write was attempted and rejected
const WRITE_REJECTED_KEY: &str = "write_rejected";

// ============================================================================
// Given Steps - Client Entity Spawning
// ============================================================================

/// Step: Given the client spawns a client-owned entity with a replicated component
/// Creates a client-owned entity with a Position component and stores its key.
#[given("the client spawns a client-owned entity with a replicated component")]
fn given_client_spawns_client_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();

    // Client spawns entity with Public replication (so it replicates to server)
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });

    // Add entity to room so it can be observed
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
        });
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    // Store initial component value for update detection
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

// ============================================================================
// When Steps - Client Write Attempts
// ============================================================================

/// Step: When the client attempts to write to the server-owned entity
/// Client attempts to modify a server-owned entity's component.
/// This write should be rejected by the server (not accepted).
#[when("the client attempts to write to the server-owned entity")]
fn when_client_attempts_write_to_server_owned_entity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Record original server value before client attempt
    let original_value = scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(entity) = server.entity(&entity_key) {
                if let Some(pos) = entity.component::<Position>() {
                    return (*pos.x, *pos.y);
                }
            }
            (0.0, 0.0)
        })
    });

    // Client attempts to modify the component
    // For server-owned entities, the client cannot directly write.
    // The client can try to modify its local copy, but the server won't accept it.
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    // Attempt to write different values
                    *pos.x = 999.0;
                    *pos.y = 888.0;
                }
            }
        });
    });

    // Advance ticks to allow replication to process
    for _ in 0..20 {
        scenario.mutate(|_| {});
    }

    // Check if server state remained unchanged (write was rejected)
    let final_value = scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(entity) = server.entity(&entity_key) {
                if let Some(pos) = entity.component::<Position>() {
                    return (*pos.x, *pos.y);
                }
            }
            (0.0, 0.0)
        })
    });

    // If original value equals final value, the write was rejected
    let write_rejected = (original_value.0 - final_value.0).abs() < f32::EPSILON
        && (original_value.1 - final_value.1).abs() < f32::EPSILON;

    scenario.bdd_store(WRITE_REJECTED_KEY, write_rejected);
}

/// Step: When the client updates the replicated component
/// Client updates its own entity's replicated component.
#[when("the client updates the replicated component")]
fn when_client_updates_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Update to a new position value
    let new_value = (100.0_f32, 200.0_f32);

    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = new_value.0;
                    *pos.y = new_value.1;
                }
            }
        });
    });

    // Store the new value for later verification
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, new_value);

    // Advance ticks to let replication happen
    for _ in 0..10 {
        scenario.mutate(|_| {});
    }
}

// ============================================================================
// Then Steps - Ownership Assertions
// ============================================================================

/// Step: Then the entity owner is the client
///
/// Polls until the client entity is present and reports EntityOwner::Client.
/// Covers [entity-ownership-02.t1]: client-owned entity MUST report Client ownership
/// on the owning client's side.
#[then("the entity owner is the client")]
fn then_entity_owner_is_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.owner() {
                EntityOwner::Client(_) => AssertOutcome::Passed(()),
                other => AssertOutcome::Failed(format!(
                    "Expected EntityOwner::Client for owned entity, got {:?}",
                    other
                )),
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server no longer has the entity
///
/// Polls until the entity is absent from the server's world.
/// Covers [entity-ownership-08.t1]: owner disconnect despawns all client-owned entities.
#[then("the server no longer has the entity")]
fn then_server_no_longer_has_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| {
        if server.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Step: Then the entity owner is the server
/// Verifies the entity is owned by the server.
#[then("the entity owner is the server")]
fn then_entity_owner_is_server(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if entity.owner() == EntityOwner::Server {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Failed(format!(
                    "Expected entity owner to be Server, but was {:?}",
                    entity.owner()
                ))
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the write is rejected
/// Verifies the unauthorized write was rejected.
#[then("the write is rejected")]
fn then_write_is_rejected(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let write_rejected: bool = ctx.scenario().bdd_get(WRITE_REJECTED_KEY).unwrap_or(false);

    if write_rejected {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(
            "Expected write to be rejected, but server state was modified".to_string(),
        )
    }
}

/// Step: Then the server observes the component update
/// Verifies the server sees the updated component value from client-owned entity.
/// This is a POLLING assertion - waits for the update to be received.
#[then("the server observes the component update")]
fn then_server_observes_component_update(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected_value: (f32, f32) = ctx
        .scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");

    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
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
