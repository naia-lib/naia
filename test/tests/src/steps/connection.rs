//! Step bindings for Connection Lifecycle contract (01_connection_lifecycle.spec.md)
//!
//! Slice 1: Event ordering & basic semantics
//! Obligations covered:
//!   - connection-24: Server event ordering with auth (AuthEvent → ConnectEvent → DisconnectEvent)
//!   - connection-25: Server event ordering without auth (ConnectEvent → DisconnectEvent)
//!   - connection-26: Client event ordering (ConnectEvent → DisconnectEvent)
//!   - connection-19: Rejected client emits RejectEvent, not ConnectEvent/DisconnectEvent
//!   - connection-21: Client DisconnectEvent only after ConnectEvent
//!   - connection-22: Server DisconnectEvent only after ConnectEvent

use std::time::Duration;

use namako::{given, when, then};
use namako::codegen::AssertOutcome;
use naia_test_harness::{
    protocol, Auth,
    ServerAuthEvent, ServerConnectEvent,
    TrackedServerEvent, TrackedClientEvent,
    ClientConnectEvent, ClientRejectEvent,
};
use naia_server::ServerConfig;
use naia_client::{ClientConfig, JitterBufferType};

use crate::{TestWorldMut, TestWorldRef};

// ============================================================================
// Given Steps - Server Setup
// ============================================================================

/// Step: Given a server is running with auth required
/// Sets up a server that requires authentication before accepting connections.
#[given("a server is running with auth required")]
fn given_server_running_with_auth(ctx: &mut TestWorldMut) {
    let scenario = ctx.init();
    let test_protocol = protocol();

    // Default config requires auth
    let server_config = ServerConfig::default();
    scenario.server_start(server_config, test_protocol);

    // Create a room for clients and store it
    let room_key = scenario.mutate(|ctx| {
        ctx.server(|server| server.make_room().key())
    });
    scenario.set_last_room(room_key);

    // Clear any previous event history
    scenario.clear_event_history();
}

// ============================================================================
// When Steps - Client Actions
// ============================================================================

/// Step: When a client authenticates and connects
/// Client goes through full auth flow and connects successfully.
/// Tracks server-side AuthEvent and ConnectEvent, plus client-side ConnectEvent.
#[when("a client authenticates and connects")]
fn when_client_authenticates_and_connects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    // Configure client for immediate handshake
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "TestClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Wait for auth event and accept connection - track AuthEvent
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Auth);

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    // Wait for server connect event - track ConnectEvent
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);

    // Need mutate between expect calls to satisfy harness constraint
    scenario.mutate(|_| {});

    // Wait for client connect event - track ClientEvent::Connect
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    // Add to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    scenario.allow_flexible_next();
}

/// Step: When a client attempts to connect but is rejected
/// Client attempts to connect but server rejects the connection.
#[when("a client attempts to connect but is rejected")]
fn when_client_attempts_connection_rejected(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();

    // Configure client for immediate handshake
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "RejectedClient",
        Auth::new("bad_user", "bad_password"),
        client_config,
        test_protocol,
    );

    // Wait for auth event but REJECT the connection
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
                }
            }
            None
        })
    });

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.reject_connection(&client_key);
        });
    });

    // Wait for client reject event - track ClientEvent::Reject
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientRejectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Reject);

    scenario.allow_flexible_next();
}

// ============================================================================
// Then Steps - Assertions
// ============================================================================

/// Step: Then the server observes AuthEvent before ConnectEvent
/// Verifies the correct ordering of server-side events per connection-24.
#[then("the server observes AuthEvent before ConnectEvent")]
fn then_server_auth_before_connect(ctx: &TestWorldRef) {
    assert!(
        ctx.server_event_before(TrackedServerEvent::Auth, TrackedServerEvent::Connect),
        "Server events out of order: expected AuthEvent before ConnectEvent. History: {:?}",
        ctx.server_event_history()
    );
}

/// Step: Then the server observes DisconnectEvent after ConnectEvent
/// Verifies connection-22: Server DisconnectEvent only after ConnectEvent.
#[then("the server observes DisconnectEvent after ConnectEvent")]
fn then_server_disconnect_after_connect(ctx: &TestWorldRef) {
    assert!(
        ctx.server_event_before(TrackedServerEvent::Connect, TrackedServerEvent::Disconnect),
        "Server events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
        ctx.server_event_history()
    );
}

/// Step: Then the client observes ConnectEvent
/// Verifies client received ConnectEvent.
/// This is a POLLING assertion - waits for event to be observed.
#[then("the client observes ConnectEvent")]
fn then_client_observes_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    if ctx.client_observed(client_key, TrackedClientEvent::Connect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Step: Then the client is connected
/// Verifies client connection status is connected.
/// This is a POLLING assertion - waits for client to become connected.
#[then("the client is connected")]
fn then_client_is_connected(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    if ctx.client_is_connected(client_key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Step: Then the client observes DisconnectEvent after ConnectEvent
/// Verifies connection-21: Client DisconnectEvent only after ConnectEvent.
/// This is a POLLING assertion - waits for DisconnectEvent, then verifies order.
#[then("the client observes DisconnectEvent after ConnectEvent")]
fn then_client_disconnect_after_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    // First: wait until Disconnect is observed (polling condition)
    if !ctx.client_observed(client_key, TrackedClientEvent::Disconnect) {
        return AssertOutcome::Pending;
    }

    // Then: verify Connect came first (hard assertion)
    if ctx.client_event_before(client_key, TrackedClientEvent::Connect, TrackedClientEvent::Disconnect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Client events out of order: expected ConnectEvent before DisconnectEvent. History: {:?}",
            ctx.client_event_history(client_key)
        ))
    }
}

/// Step: Then the client is not connected
/// Verifies client connection status is not connected.
/// This is a POLLING assertion - waits for client to become disconnected.
#[then("the client is not connected")]
fn then_client_is_not_connected(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    if !ctx.client_is_connected(client_key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Step: Then the client observes RejectEvent
/// Verifies client received RejectEvent per connection-19.
/// This is a POLLING assertion - waits for event to be observed.
#[then("the client observes RejectEvent")]
fn then_client_observes_reject(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();

    if ctx.client_observed(client_key, TrackedClientEvent::Reject) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Step: Then the client does not observe ConnectEvent
/// Verifies client did NOT receive ConnectEvent (for rejection scenarios).
#[then("the client does not observe ConnectEvent")]
fn then_client_no_connect(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    assert!(
        !ctx.client_observed(client_key, TrackedClientEvent::Connect),
        "Client should NOT have observed ConnectEvent but did. History: {:?}",
        ctx.client_event_history(client_key)
    );
}

/// Step: Then the client does not observe DisconnectEvent
/// Verifies client did NOT receive DisconnectEvent (for rejection scenarios).
#[then("the client does not observe DisconnectEvent")]
fn then_client_no_disconnect(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();

    assert!(
        !ctx.client_observed(client_key, TrackedClientEvent::Disconnect),
        "Client should NOT have observed DisconnectEvent but did. History: {:?}",
        ctx.client_event_history(client_key)
    );
}
