//! Step bindings for Scope Exit Policy contract (15_scope_exit_policy.spec.md)
//!
//! New behavior:
//!   - ScopeExit::Persist: entity stays on client when scope is lost; updates frozen
//!   - Accumulated deltas are delivered on re-entry
//!   - Global despawn while Paused propagates to client
//!   - Component insert/remove during absence applied on re-entry
//!   - Disconnect while Paused does not crash

use naia_server::ReplicationConfig;
use naia_test_harness::{EntityKey, ImmutableLabel, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";
// Initial position value stored at spawn for stale-value assertion
const INITIAL_POSITION: (f32, f32) = (0.0, 0.0);
// Expected position after server update
const UPDATED_POSITION: (f32, f32) = (100.0, 100.0);

// ============================================================================
// Given Steps — Entity Setup
// ============================================================================

/// Step: Given a server-owned entity exists with ScopeExit::Persist configured
/// Creates a server entity with Position(0,0) and sets ScopeExit::Persist.
#[given("a server-owned entity exists with ScopeExit::Persist configured")]
fn given_persist_entity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(INITIAL_POSITION.0, INITIAL_POSITION.1))
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Step: Given a server-owned entity exists with ScopeExit::Persist configured without ImmutableLabel
/// Creates a server entity with Position only (no ImmutableLabel) and sets ScopeExit::Persist.
#[given("a server-owned entity exists with ScopeExit::Persist configured without ImmutableLabel")]
fn given_persist_entity_without_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(INITIAL_POSITION.0, INITIAL_POSITION.1))
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Step: Given a server-owned entity exists with ScopeExit::Persist configured with ImmutableLabel
/// Creates a server entity with Position AND ImmutableLabel and sets ScopeExit::Persist.
#[given("a server-owned entity exists with ScopeExit::Persist configured with ImmutableLabel")]
fn given_persist_entity_with_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(INITIAL_POSITION.0, INITIAL_POSITION.1))
                    .insert_component(ImmutableLabel)
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });

    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

// ============================================================================
// When Steps — Tick Advance, Position Update, Component Ops, Despawn
// ============================================================================

/// Step: When the server advances N ticks
/// Runs exactly N server ticks without any other mutations — used to bound
/// "no update should arrive in N ticks" window for stale-value assertions.
#[when("the server advances {int} ticks")]
fn when_server_advances_n_ticks(ctx: &mut TestWorldMut, n: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..n {
        scenario.mutate(|_| {});
    }
}

/// Step: When the server updates the entity position to 100 100
/// Sets the entity's Position to (100, 100). Used to test that this update
/// does NOT reach the client while excluded, but DOES arrive on re-entry.
#[when("the server updates the entity position to 100 100")]
fn when_server_updates_entity_position(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = UPDATED_POSITION.0;
                    *pos.y = UPDATED_POSITION.1;
                }
            }
        });
    });

    scenario.mutate(|_| {});
}

/// Step: When the server globally despawns the entity
/// Despawns the entity from the server world entirely. The client should
/// receive a Despawn even if the entity was Paused (ScopeExit::Persist).
#[when("the server globally despawns the entity")]
fn when_server_globally_despawns_entity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.despawn();
            }
        });
    });

    scenario.mutate(|_| {});
}

/// Step: When the server inserts ImmutableLabel on the entity
/// Inserts ImmutableLabel while the entity is excluded (Paused). Should appear
/// on the client's entity after the entity re-enters scope.
#[when("the server inserts ImmutableLabel on the entity")]
fn when_server_inserts_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.insert_component(ImmutableLabel);
            }
        });
    });

    scenario.mutate(|_| {});
}

/// Step: When the server removes ImmutableLabel from the entity
/// Removes ImmutableLabel while the entity is excluded (Paused). Should be
/// absent on the client's entity after the entity re-enters scope.
#[when("the server removes ImmutableLabel from the entity")]
fn when_server_removes_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.remove_component::<ImmutableLabel>();
            }
        });
    });

    scenario.mutate(|_| {});
}

// ============================================================================
// Then Steps — Assertions
// ============================================================================

/// Step: Then the client still has the entity
/// Immediate assertion that the entity is present in the client's networked
/// entity pool. Used to confirm ScopeExit::Persist prevented the Despawn.
#[then("the client still has the entity")]
fn then_client_still_has_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Failed(
                "Entity was despawned on client despite ScopeExit::Persist".into(),
            )
        }
    })
}

/// Step: Then the client entity position is still 0.0
/// Immediate assertion that the entity's Position x-component is 0.0.
/// Confirms no update leaked through while the entity was Paused.
/// Fails immediately if position has been updated (update leaked through).
#[then("the client entity position is still 0.0")]
fn then_client_entity_position_still_zero(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Failed(
                "Entity absent on client despite ScopeExit::Persist".into(),
            );
        };
        let Some(pos) = entity.component::<Position>() else {
            // Position not yet synced — unusual but not a failure yet
            return AssertOutcome::Pending;
        };
        let x = *pos.x;
        if (x - INITIAL_POSITION.0).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Failed(format!(
                "Position updated while entity was out-of-scope: expected x={}, got x={x}",
                INITIAL_POSITION.0,
            ))
        }
    })
}

/// Step: Then the client entity position becomes 100.0
/// Polling assertion that the entity's Position x-component eventually reaches 100.0.
/// Confirms accumulated updates from the Paused period arrive after re-entry.
#[then("the client entity position becomes 100.0")]
fn then_client_entity_position_becomes_hundred(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        let x = *pos.x;
        if (x - UPDATED_POSITION.0).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client entity has ImmutableLabel
/// Polling assertion that ImmutableLabel is present on the client-side entity.
#[then("the client entity has ImmutableLabel")]
fn then_client_entity_has_label(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        if entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client entity does not have ImmutableLabel
/// Polling assertion that ImmutableLabel is absent on the client-side entity.
/// Uses polling to allow the re-entry sync to complete.
#[then("the client entity does not have ImmutableLabel")]
fn then_client_entity_no_label(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            // Entity not present at all — treat as Pending to give re-entry time
            return AssertOutcome::Pending;
        };
        if !entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            // Still has it — re-entry remove delta not yet applied
            AssertOutcome::Pending
        }
    })
}
