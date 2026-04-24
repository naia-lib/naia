use naia_test_harness::EntityKey;
use namako_engine::codegen::AssertOutcome;
use namako_engine::{then, when, given};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// Contract 17 — Update Candidate Set Model (Phase 3)
// ============================================================================

/// Advances one tick without any mutations.
/// Used to confirm idle ticks produce no dirty update candidates.
#[when("the server performs an idle tick")]
fn when_server_performs_idle_tick(ctx: &mut TestWorldMut) {
    ctx.scenario_mut().mutate(|_| {});
}

/// Spawns the stored entity into a room the client is NOT in.
/// Ensures no shared-room relationship exists between the entity and client.
#[given("the entity is not in the client's room")]
fn given_entity_not_in_clients_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    // Put the entity in a separate room (not the client's room).
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let separate_room = server.make_room().key();
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&separate_room);
            }
        });
    });
}

/// Asserts that the total dirty update candidate count across all server
/// connections is 0 after the last tick.
///
/// Returns 0 on the legacy (non-Phase-3) path — there is no dirty set.
/// Returns 0 on the Phase 3 path when the dirty set drained cleanly.
#[then("the total dirty update candidate count is 0")]
fn then_total_dirty_update_candidate_count_is_zero(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let count = ctx.scenario().total_dirty_update_count();
    if count == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected total dirty update candidate count 0, got {}",
            count
        ))
    }
}
