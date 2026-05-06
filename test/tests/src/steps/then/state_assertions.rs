//! Then-step bindings: observable state predicates.
//!
//! State assertions check the system's *current* observable state —
//! number of connected clients, which entities a client sees, what
//! authority status a client holds, etc. Distinct from
//! [`event_assertions`](super::event_assertions) which assert on
//! the *history* of emitted events.

use namako_engine::then;

use crate::TestWorldRef;

/// Then the server has {int} connected client(s).
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(ctx: &TestWorldRef, expected: usize) {
    let scenario = ctx.scenario();
    let count = scenario.server().expect("server").users_count();
    assert_eq!(
        count, expected,
        "server should have {} connected clients",
        expected
    );
}

/// Then the system intentionally fails.
///
/// Demo step from the P0-A runtime-failure scaffolding. Always
/// panics. Kept here for the namako-runtime smoke check.
#[then("the system intentionally fails")]
fn then_system_intentionally_fails(_ctx: &TestWorldRef) {
    panic!("INTENTIONAL FAILURE: This step is designed to fail for demo purposes");
}

// ──────────────────────────────────────────────────────────────────────
// Server-side instrumentation snapshots
// ──────────────────────────────────────────────────────────────────────

/// Then the scope change queue depth is 0.
///
/// Asserts that the server's scope-change queue drained cleanly after
/// the last tick. Used by scope-propagation tests.
#[then("the scope change queue depth is 0")]
fn then_scope_change_queue_depth_is_zero(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let depth = ctx.scenario().scope_change_queue_len();
    if depth == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Expected scope_change_queue depth 0, got {}",
            depth
        ))
    }
}

/// Then the total dirty update candidate count is 0.
///
/// Asserts that the per-tick dirty-update set drained cleanly. Used
/// by update-candidate-set tests.
#[then("the total dirty update candidate count is 0")]
fn then_total_dirty_update_candidate_count_is_zero(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let count = ctx.scenario().total_dirty_update_count();
    if count == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Expected total dirty update candidate count 0, got {}",
            count
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Diff-handler receiver-count assertions (immutable-component tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the global diff handler has 0 receivers.
#[then("the global diff handler has 0 receivers")]
fn then_global_diff_handler_has_zero_receivers(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Expected 0 global diff-handler receivers, got {}",
            snapshot.global_receivers
        ))
    }
}

/// Then the global diff handler has 1 receiver.
#[then("the global diff handler has 1 receiver")]
fn then_global_diff_handler_has_one_receiver(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 1 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Expected 1 global diff-handler receiver, got {}",
            snapshot.global_receivers
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Component-replication assertions (spawn-with-components tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the entity spawns on the client with Position and Velocity.
#[then("the entity spawns on the client with Position and Velocity")]
fn then_entity_spawns_with_position_and_velocity(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position, Velocity};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() && entity.has_component::<Velocity>() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Component-presence assertions (world-integration tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the client world has the component on the entity.
///
/// Polls until the client's local entity has Position. Covers
/// [world-integration-08.t1].
#[then("the client world has the component on the entity")]
fn then_client_world_has_component_on_entity(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.has_component::<Position>() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the client world no longer has the component on the entity.
///
/// Polls until the client's local entity no longer has Position.
/// Covers [world-integration-09.t1].
#[then("the client world no longer has the component on the entity")]
fn then_client_world_no_longer_has_component(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.component::<Position>().is_none() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the second client has the entity in its world.
///
/// Covers [world-integration-05.t1] (late-joining client receives
/// current snapshot).
#[then("the second client has the entity in its world")]
fn then_second_client_has_entity_in_world(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{ClientKey, EntityKey};
    let scenario = ctx.scenario();
    let second_client: ClientKey = scenario
        .bdd_get(crate::steps::world_helpers::SECOND_CLIENT_KEY)
        .expect("second client not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(second_client, |c| {
        if c.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Authority status
// ──────────────────────────────────────────────────────────────────────

/// Then client A observes no authority status for the entity.
///
/// Covers [entity-authority-01.t1] (authority None for non-delegated).
#[then("client A observes no authority status for the entity")]
fn then_client_a_observes_no_authority_status(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{ClientKey, EntityKey};
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                None => namako_engine::codegen::AssertOutcome::Passed(()),
                Some(status) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "expected None authority for non-delegated entity, got {:?}",
                    status
                )),
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the authority request fails with an error.
///
/// Reads the `LAST_REQUEST_ERROR_KEY` boolean stored by the matching
/// When binding. Covers [entity-authority-07.t1].
#[then("the authority request fails with an error")]
fn then_authority_request_fails_with_error(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    match ctx
        .scenario()
        .bdd_get::<bool>(crate::steps::world_helpers::LAST_REQUEST_ERROR_KEY)
    {
        Some(true) => namako_engine::codegen::AssertOutcome::Passed(()),
        Some(false) => namako_engine::codegen::AssertOutcome::Failed(
            "expected request_authority to return Err for non-delegated entity, got Ok".to_string(),
        ),
        None => namako_engine::codegen::AssertOutcome::Failed("no request result stored".to_string()),
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity ownership
// ──────────────────────────────────────────────────────────────────────

/// Then the entity owner is the client.
#[then("the entity owner is the client")]
fn then_entity_owner_is_client(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, EntityOwner};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.owner() {
                EntityOwner::Client(_) => namako_engine::codegen::AssertOutcome::Passed(()),
                other => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "Expected EntityOwner::Client for owned entity, got {:?}",
                    other
                )),
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the entity owner is the server.
#[then("the entity owner is the server")]
fn then_entity_owner_is_server(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, EntityOwner};
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if entity.owner() == EntityOwner::Server {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Failed(format!(
                    "Expected entity owner to be Server, but was {:?}",
                    entity.owner()
                ))
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the server no longer has the entity.
///
/// Covers [entity-ownership-08.t1] (owner disconnect despawns).
#[then("the server no longer has the entity")]
fn then_server_no_longer_has_entity(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.server(|server| {
        if server.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Pending
        } else {
            namako_engine::codegen::AssertOutcome::Passed(())
        }
    })
}

/// Then the write is rejected.
///
/// Reads the `WRITE_REJECTED_KEY` boolean set by the matching When
/// binding. Covers [entity-ownership-02].
#[then("the write is rejected")]
fn then_write_is_rejected(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let rejected: bool = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::WRITE_REJECTED_KEY)
        .unwrap_or(false);
    if rejected {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(
            "Expected write to be rejected, but server state was modified".to_string(),
        )
    }
}

/// Then the server observes the component update.
///
/// Polls until server-side Position equals the value stored under
/// `LAST_COMPONENT_VALUE_KEY` by the matching When binding.
#[then("the server observes the component update")]
fn then_server_observes_component_update(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected: (f32, f32) = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                if (*pos.x - expected.0).abs() < f32::EPSILON
                    && (*pos.y - expected.1).abs() < f32::EPSILON
                {
                    namako_engine::codegen::AssertOutcome::Passed(())
                } else {
                    namako_engine::codegen::AssertOutcome::Pending
                }
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client with correct Position and Velocity values.
#[then("the entity spawns on the client with correct Position and Velocity values")]
fn then_entity_spawns_with_correct_values(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position, Velocity};
    let client_key = ctx.last_client();
    let scenario = ctx.scenario();
    let entity_key: EntityKey = scenario
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected_pos: (f32, f32) = scenario
        .bdd_get(crate::steps::world_helpers::SPAWN_POSITION_VALUE_KEY)
        .expect("No position value stored");
    let expected_vel: (f32, f32) = scenario
        .bdd_get(crate::steps::world_helpers::SPAWN_VELOCITY_VALUE_KEY)
        .expect("No velocity value stored");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(vel) = entity.component::<Velocity>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let pos_ok = (*pos.x - expected_pos.0).abs() < f32::EPSILON
            && (*pos.y - expected_pos.1).abs() < f32::EPSILON;
        let vel_ok = (*vel.vx - expected_vel.0).abs() < f32::EPSILON
            && (*vel.vy - expected_vel.1).abs() < f32::EPSILON;
        if pos_ok && vel_ok {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}
