//! Step bindings for basic smoke tests.
//! These steps verify the core Naia functionality works end-to-end.

use std::time::Duration;

use namako::{given, when, then};
use naia_test_harness::{
    protocol, Auth,
    ServerAuthEvent, ServerConnectEvent,
    TrackedServerEvent, TrackedClientEvent,
    ClientDisconnectEvent,
};
use naia_server::ServerConfig;
use naia_client::{ClientConfig, JitterBufferType};

use crate::{TestWorldMut, TestWorldRef};

// ============================================================================
// Given Steps - Server Setup
// ============================================================================

/// Step: Given a server is running
#[given("a server is running")]
fn given_server_running(ctx: &mut TestWorldMut) {
    let scenario = ctx.init();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol);

    // Create a room for clients and store it
    let room_key = scenario.mutate(|ctx| {
        ctx.server(|server| server.make_room().key())
    });
    scenario.set_last_room(room_key);
}

// ============================================================================
// When Steps - Client Actions
// ============================================================================

/// Step: When a client connects
#[when("a client connects")]
fn when_client_connects(ctx: &mut TestWorldMut) {
    connect_client_impl(ctx);
}

/// Step: Given a client connects (for And/But after Given)
#[given("a client connects")]
fn given_client_connects(ctx: &mut TestWorldMut) {
    connect_client_impl(ctx);
}

/// Internal implementation for client connection.
fn connect_client_impl(ctx: &mut TestWorldMut) {
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

    // Wait for auth event and accept connection
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
            server.accept_connection(&client_key);
        });
    });

    // Wait for server connect event and track it
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

    // Add client to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room exists").add_user(&client_key);
        });
    });

    // Wait for client connect event and track it
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<naia_test_harness::ClientConnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
}

/// Step: When the server disconnects the client
#[when("the server disconnects the client")]
fn when_server_disconnects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Queue the disconnect on server side
    // Server will send disconnect packets to the client
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });

    // Track server disconnect event immediately (server disconnect is synchronous)
    scenario.track_server_event(TrackedServerEvent::Disconnect);

    // Wait for client disconnect event (client should receive disconnect packet from server)
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            client.read_event::<ClientDisconnectEvent>()
        })
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);

    scenario.allow_flexible_next();
}

// ============================================================================
// Then Steps - Assertions
// ============================================================================

/// Step: Then the server has {int} connected client(s)
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(ctx: &TestWorldRef, expected: usize) {
    let scenario = ctx.scenario();
    let count = scenario.server().expect("server").users_count();
    assert_eq!(count, expected, "server should have {} connected clients", expected);
}

// ============================================================================
// DEMO: Intentional Failure Step (for P0-A Runtime Failure Demo)
// ============================================================================

/// Step: Then the system intentionally fails
#[then("the system intentionally fails")]
fn then_system_intentionally_fails(_ctx: &TestWorldRef) {
    panic!("INTENTIONAL FAILURE: This step is designed to fail for demo purposes");
}
