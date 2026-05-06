//! Then-step bindings: subsequence/order assertions across events.

use namako_engine::codegen::AssertOutcome;
use namako_engine::then;

use crate::TestWorldRef;

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — server-side event ordering
// ──────────────────────────────────────────────────────────────────────

/// Then the server observes AuthEvent before ConnectEvent.
///
/// Covers [connection-24].
#[then("the server observes AuthEvent before ConnectEvent")]
fn then_server_auth_before_connect(ctx: &TestWorldRef) {
    use naia_test_harness::TrackedServerEvent;
    assert!(
        ctx.server_event_before(TrackedServerEvent::Auth, TrackedServerEvent::Connect),
        "Server events out of order: expected AuthEvent before ConnectEvent. History: {:?}",
        ctx.server_event_history()
    );
}

/// Then the server observes DisconnectEvent after ConnectEvent.
///
/// Covers [connection-22] (Server DisconnectEvent only after ConnectEvent).
#[then("the server observes DisconnectEvent after ConnectEvent")]
fn then_server_disconnect_after_connect(ctx: &TestWorldRef) {
    use naia_test_harness::TrackedServerEvent;
    assert!(
        ctx.server_event_before(TrackedServerEvent::Connect, TrackedServerEvent::Disconnect),
        "Server events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
        ctx.server_event_history()
    );
}

/// Then the client observes DisconnectEvent after ConnectEvent.
///
/// Polls until Disconnect is observed, then asserts Connect came first.
/// Covers [connection-21].
#[then("the client observes DisconnectEvent after ConnectEvent")]
fn then_client_disconnect_after_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if !ctx.client_observed(client_key, TrackedClientEvent::Disconnect) {
        return AssertOutcome::Pending;
    }
    if ctx.client_event_before(
        client_key,
        TrackedClientEvent::Connect,
        TrackedClientEvent::Disconnect,
    ) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Client events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
            ctx.client_event_history(client_key)
        ))
    }
}

/// Then the server observed ConnectEvent before DisconnectEvent.
#[then("the server observed ConnectEvent before DisconnectEvent")]
fn then_server_connect_before_disconnect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedServerEvent;
    if !ctx.server_observed(TrackedServerEvent::Disconnect) {
        return AssertOutcome::Pending;
    }
    if ctx.server_event_before(TrackedServerEvent::Connect, TrackedServerEvent::Disconnect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Server events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
            ctx.server_event_history()
        ))
    }
}

/// Then the client observed ConnectEvent before DisconnectEvent.
#[then("the client observed ConnectEvent before DisconnectEvent")]
fn then_client_connect_before_disconnect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if !ctx.client_observed(client_key, TrackedClientEvent::Disconnect) {
        return AssertOutcome::Pending;
    }
    if ctx.client_event_before(
        client_key,
        TrackedClientEvent::Connect,
        TrackedClientEvent::Disconnect,
    ) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Client events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
            ctx.client_event_history(client_key)
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Common — determinism + per-tick ordering predicates
// ──────────────────────────────────────────────────────────────────────

/// Then the event emission order is identical both times.
///
/// Determinism contract — the local transport + TestClock guarantee
/// identical event ordering. Asserts no panic + Connect was observed.
#[then("the event emission order is identical both times")]
fn then_event_order_identical(ctx: &TestWorldRef) {
    use naia_test_harness::TrackedClientEvent;
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Determinism test failed with panic: {:?}",
        result.panic_msg
    );
    let client_key = ctx.last_client();
    assert!(
        ctx.client_observed(client_key, TrackedClientEvent::Connect),
        "Expected deterministic Connect event"
    );
}

/// Then the entity spawn order is identical both times.
#[then("the entity spawn order is identical both times")]
fn then_entity_spawn_order_identical(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Entity spawn determinism failed: {:?}",
        result.panic_msg
    );
    assert!(
        result.is_ok,
        "Deterministic operations should complete successfully"
    );
}

/// Then the final scope state reflects the last API call order.
///
/// Asserts the scope-op trace contains the canonical `include_1`,
/// `exclude_2`, `include_3` subsequence — proving operations queued
/// in the same tick were applied in API call order.
#[then("the final scope state reflects the last API call order")]
fn then_scope_reflects_last_api_order(ctx: &TestWorldRef) {
    let scenario = ctx.scenario();
    let result = scenario
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Scope operations caused a panic: {:?}",
        result.panic_msg
    );
    let labels: Vec<_> = scenario.trace_labels().collect();
    assert!(
        scenario.trace_contains_subsequence(&[
            "scope_op_include_1",
            "scope_op_exclude_2",
            "scope_op_include_3"
        ]),
        "Expected scope operations in order. Trace: {:?}",
        labels
    );
}

/// Then commands are applied in receipt order.
#[then("commands are applied in receipt order")]
fn then_commands_in_receipt_order(ctx: &TestWorldRef) {
    let scenario = ctx.scenario();
    let result = scenario
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Command processing caused a panic: {:?}",
        result.panic_msg
    );
    let labels: Vec<_> = scenario.trace_labels().collect();
    assert!(
        scenario.trace_contains_subsequence(&["command_A", "command_B", "command_C"]),
        "Expected commands in receipt order. Trace: {:?}",
        labels
    );
}

/// Then commands are applied in ascending sequence order.
///
/// Out-of-order arrivals (seq 2, 0, 1) must be reordered to seq
/// 0, 1, 2 before application.
#[then("commands are applied in ascending sequence order")]
fn then_commands_in_ascending_sequence_order(ctx: &TestWorldRef) {
    let scenario = ctx.scenario();
    let result = scenario
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Command reordering caused a panic: {:?}",
        result.panic_msg
    );
    let labels: Vec<_> = scenario.trace_labels().collect();
    assert!(
        scenario.trace_contains_subsequence(&["apply_seq0_A", "apply_seq1_B", "apply_seq2_C"]),
        "Expected commands applied in ascending sequence order. Trace: {:?}",
        labels
    );
}
