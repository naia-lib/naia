use naia_test_harness::{ImmutableLabel, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// Contract 19 — Immutable Replicated Components (Phase 5)
// ============================================================================

/// Spawn a server-owned entity carrying only `ImmutableLabel`.
/// Stores the entity key under LAST_ENTITY_KEY for downstream steps.
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

/// Spawn a server-owned entity carrying `Position` (mutable) **and** `ImmutableLabel`.
/// Stores the entity key under LAST_ENTITY_KEY for downstream steps.
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

/// Assert that the global diff handler has exactly 0 receiver registrations.
/// Passes immediately (no polling) since diff-handler state is set at scope-entry time.
#[then("the global diff handler has 0 receivers")]
fn then_global_diff_handler_has_zero_receivers(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected 0 global diff-handler receivers, got {}",
            snapshot.global_receivers
        ))
    }
}

/// Assert that the global diff handler has exactly 1 receiver registration.
/// Passes immediately since diff-handler state is set at scope-entry time.
#[then("the global diff handler has 1 receiver")]
fn then_global_diff_handler_has_one_receiver(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 1 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected 1 global diff-handler receiver, got {}",
            snapshot.global_receivers
        ))
    }
}
