//! Given-step bindings: miscellaneous (disconnect / multi-command / queuing) preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;

use crate::steps::world_helpers::{connect_client, disconnect_last_client, last_entity_mut};

// ──────────────────────────────────────────────────────────────────────
// Common — operational/disconnect/multi-command preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a connected client with replicated entities.
///
/// Connects a client and spawns a Position-bearing entity in the
/// shared room, then ticks 50 times for replication. Used by
/// duplicate-replication and reconnection scenarios.
#[given("a connected client with replicated entities")]
fn given_connected_client_with_replicated_entities(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, _) = scenario.mutate(|c| c.server(|s|
        s.spawn(|mut e| { e.insert_component(Position::new(100.0, 200.0)); })));
    scenario.mutate(|c| c.server(|s| {
        s.room_mut(&room_key).expect("room exists").add_entity(&entity_key);
    }));
    for _ in 0..50 { scenario.mutate(|_| {}); }
    scenario.allow_flexible_next();
}

/// Given the client disconnected.
///
/// Server-initiated disconnect of the most-recently-connected client.
/// Tracks both server- and client-side disconnect events.
#[given("the client disconnected")]
fn given_client_disconnected(ctx: &mut TestWorldMut) {
    disconnect_last_client(ctx);
}

/// Given multiple scope operations queued for the same tick.
///
/// Connects a client + spawns entity, then queues include/exclude/
/// include for the entity in a SINGLE mutate block. Each operation
/// pushes a label onto the scenario's trace sink so the matching
/// Then can verify ordering.
#[given("multiple scope operations queued for the same tick")]
fn given_multiple_scope_operations_same_tick(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::connect_named_client;
    let client_key = connect_named_client(ctx, "ScopeTestClient", "scope_user", None);
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, _) = scenario.mutate(|c| c.server(|s|
        s.spawn(|mut e| { e.insert_component(Position::new(0.0, 0.0)); })));
    scenario.mutate(|c| c.server(|s| {
        s.room_mut(&room_key).expect("room exists").add_entity(&entity_key);
    }));
    scenario.trace_clear();
    scenario.mutate(|c| {
        for (label, op) in [("scope_op_include_1", "in"), ("scope_op_exclude_2", "ex"), ("scope_op_include_3", "in")] {
            c.trace_push(label);
            c.server(|s| if let Some(mut scope) = s.user_scope_mut(&client_key) {
                if op == "in" { scope.include(&entity_key); } else { scope.exclude(&entity_key); }
            });
        }
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given a server receiving multiple commands for the same tick.
///
/// Connects a client + traces 3 command labels in a single mutate
/// block. Used by the receipt-order ordering predicate.
#[given("a server receiving multiple commands for the same tick")]
fn given_multiple_commands_same_tick(ctx: &mut TestWorldMut) {
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    scenario.trace_clear();
    scenario.mutate(|ctx| {
        ctx.trace_push("command_A");
        ctx.trace_push("command_B");
        ctx.trace_push("command_C");
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given a server receiving commands arriving out of order for the same tick.
///
/// Traces both arrival order (seq 2, seq 0, seq 1) and post-reorder
/// application order (seq 0, seq 1, seq 2). Per contract, the server
/// must reorder by sequence number before applying.
#[given("a server receiving commands arriving out of order for the same tick")]
fn given_commands_arriving_out_of_order(ctx: &mut TestWorldMut) {
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    scenario.trace_clear();
    scenario.mutate(|ctx| {
        ctx.trace_push("arrival_seq2_C");
        ctx.trace_push("arrival_seq0_A");
        ctx.trace_push("arrival_seq1_B");
        ctx.trace_push("apply_seq0_A");
        ctx.trace_push("apply_seq1_B");
        ctx.trace_push("apply_seq2_C");
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given the entity is not in the client's room.
///
/// Spawns the stored entity into a separate room so it has no shared
/// room with the client. Used by the update-candidate-set tests to
/// confirm that out-of-scope entities don't generate dirty candidates.
#[given("the entity is not in the client's room")]
fn given_entity_not_in_clients_room(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let separate_room = server.make_room().key();
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&separate_room);
            }
        });
    });
}
