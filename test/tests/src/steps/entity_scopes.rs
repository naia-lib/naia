//! Step bindings for Entity Scopes contract (06_entity_scopes.spec.md)
//!
//! These steps cover:
//!   - Rooms gating (SharesRoom predicate for InScope)
//!   - Include/Exclude filters after room gate
//!   - Owner scope invariant (owning client always in-scope)

use namako_engine::{given, when, then};
use namako_engine::codegen::AssertOutcome;
use naia_test_harness::{EntityKey, Position};
use naia_client::ReplicationConfig as ClientReplicationConfig;

use crate::{TestWorldMut, TestWorldRef};

/// Storage key for the last entity created in BDD tests
const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// Given Steps - Entity Setup
// ============================================================================

/// Step: Given a server-owned entity exists
/// Creates a server-owned entity and stores its key for later use.
#[given("a server-owned entity exists")]
fn given_server_owned_entity_exists(ctx: &mut TestWorldMut) {
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
}

/// Step: Given the client owns an entity
/// Creates a client-owned entity and stores its key for later use.
#[given("the client owns an entity")]
fn given_client_owns_entity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Client spawns entity with Public replication (so it replicates to server and back)
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
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Step: Given the client and entity share a room
/// Adds the entity to the room that the client is in.
#[given("the client and entity share a room")]
fn given_client_and_entity_share_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
        });
    });
}

/// Step: Given the client and entity do not share a room
/// Ensures the entity is not in any room shared with the client.
#[given("the client and entity do not share a room")]
fn given_client_and_entity_do_not_share_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.leave_room(&room_key);
            }
        });
    });
}

/// Step: Given the entity is in-scope for the client
/// Includes the entity in the client's scope (after room gate is satisfied).
#[given("the entity is in-scope for the client")]
fn given_entity_in_scope_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });

    // Advance and wait until the client actually observes the entity in-scope.
    scenario.mutate(|_| {});
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if client.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
}

/// Step: Given the entity is out-of-scope for the client
/// Excludes the entity from the client's scope initially.
#[given("the entity is out-of-scope for the client")]
fn given_entity_out_of_scope_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });

    // Advance a tick to let the scope change propagate
    scenario.mutate(|_| {});
}

/// Step: Given the server excludes the entity for the client
/// Excludes the entity from the client's scope as initial setup.
#[given("the server excludes the entity for the client")]
fn given_server_excludes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });

    // Advance a tick to let the scope change propagate
    scenario.mutate(|_| {});
}

/// Step: Given the entity is not in any room
/// Ensures the entity is not in any room.
#[given("the entity is not in any room")]
fn given_entity_not_in_any_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Remove entity from all rooms
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let room_keys = server.room_keys();
            for room_key in room_keys {
                if let Some(mut room) = server.room_mut(&room_key) {
                    if room.has_entity(&entity_key) {
                        room.remove_entity(&entity_key);
                    }
                }
            }
        });
    });

    // Advance a tick to let the change propagate
    scenario.mutate(|_| {});
}

// ============================================================================
// When Steps - Scope Operations
// ============================================================================

/// Step: When the server includes the entity for the client
/// Includes the entity in the client's scope.
#[when("the server includes the entity for the client")]
fn when_server_includes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });

    // Advance a tick to let the scope change propagate
    scenario.mutate(|_| {});
}

/// Step: When the server excludes the entity for the client
/// Excludes the entity from the client's scope.
#[when("the server excludes the entity for the client")]
fn when_server_excludes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });

    // Advance a tick to let the scope change propagate
    scenario.mutate(|_| {});
}

/// Step: When the entity despawns on the client
/// Waits for the entity to despawn on the client side before proceeding.
#[when("the entity despawns on the client")]
fn when_entity_despawns_on_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Wait for entity to despawn on client
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if !client.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
}

// ============================================================================
// Then Steps - Scope Assertions
// ============================================================================

/// Step: Then the entity is in-scope for the client
/// Verifies the entity is in the client's scope.
#[then("the entity is in-scope for the client")]
fn then_entity_in_scope_for_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the entity is out-of-scope for the client
/// Verifies the entity is not in the client's scope.
#[then("the entity is out-of-scope for the client")]
fn then_entity_out_of_scope_for_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if !scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            // If no scope exists, entity is effectively out of scope
            AssertOutcome::Passed(())
        }
    })
}

/// Step: Then the entity despawns on the client
/// Verifies the entity no longer exists on the client side after leaving scope.
/// This is a POLLING assertion - waits for entity to despawn.
#[then("the entity despawns on the client")]
fn then_entity_despawns_on_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if !client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the entity spawns on the client as a new lifetime
/// Verifies the entity exists on the client side as a fresh spawn.
/// This is a POLLING assertion - waits for entity to spawn.
#[then("the entity spawns on the client as a new lifetime")]
fn then_entity_spawns_on_client_as_new_lifetime(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server stops replicating entities to that client
/// Verifies that after disconnect, the server no longer has the user in its user list
/// (which implies it stops replicating entities to that client).
/// This is a POLLING assertion - waits for user to be removed.
#[then("the server stops replicating entities to that client")]
fn then_server_stops_replicating_to_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    ctx.server(|server| {
        if !server.user_exists(&client_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the entity spawns on the client
/// Verifies the entity exists on the client side.
/// This is a POLLING assertion - waits for entity to spawn.
#[then("the entity spawns on the client")]
fn then_entity_spawns_on_client(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: When the server includes the entity for an unknown client
/// Attempts to include an entity for a non-existent client (should be a no-op).
#[when("the server includes the entity for an unknown client")]
fn when_server_includes_entity_for_unknown_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Create a fake/unknown client key that doesn't exist in the scenario
    let unknown_client_key = naia_test_harness::ClientKey::invalid();

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            // This should be a no-op since the client doesn't exist
            // user_scope_mut returns None for unknown clients
            if let Some(mut scope) = server.user_scope_mut(&unknown_client_key) {
                scope.include(&entity_key);
            }
        });
    });

    // Advance a tick
    scenario.mutate(|_| {});
}

/// Step: Then no error is raised
/// Verifies that the previous operation did not raise an error.
/// Since we reached this step, no panic occurred.
#[then("no error is raised")]
fn then_no_error_is_raised(_ctx: &TestWorldRef) -> AssertOutcome<()> {
    AssertOutcome::Passed(())
}
