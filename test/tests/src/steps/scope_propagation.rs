use naia_test_harness::EntityKey;
use namako_engine::codegen::AssertOutcome;
use namako_engine::{then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// Contract 16 — Scope Propagation Model (Phase 2)
// ============================================================================

/// Adds the stored entity to the room the client is already in.
/// Used for scope-propagation-01-c: entity added to room while user is present.
#[when("the server adds the entity to the client's room")]
fn when_server_adds_entity_to_room(ctx: &mut TestWorldMut) {
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

/// Asserts that the server's scope-change queue depth is 0 after the last tick.
///
/// Returns 0 on the legacy (non-v2_push_pipeline) path — there is no queue.
/// Returns 0 on the v2_push_pipeline path when the queue drained cleanly.
#[then("the scope change queue depth is 0")]
fn then_scope_change_queue_depth_is_zero(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let depth = ctx.scenario().scope_change_queue_len();
    if depth == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected scope_change_queue depth 0, got {}",
            depth
        ))
    }
}
