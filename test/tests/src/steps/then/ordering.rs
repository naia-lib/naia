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
