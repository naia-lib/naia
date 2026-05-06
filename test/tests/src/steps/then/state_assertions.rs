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
