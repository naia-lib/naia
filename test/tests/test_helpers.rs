use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::RoomKey;
use naia_shared::Protocol;
use naia_test::{
    Scenario, ClientKey,
    protocol, Auth,
    AuthEvent, ConnectEvent,
};

// ============================================================================
// Connection Helpers
// ============================================================================

/// Assert that a client is connected (both client-side and server-side)
pub fn assert_connected(scenario: &mut Scenario, client_key: ClientKey) {
    scenario.expect(|ctx| {
        let connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        (connected && user_exists).then_some(())
    });
}

/// Assert that a client is NOT connected
pub fn assert_disconnected(scenario: &mut Scenario, client_key: ClientKey) {
    scenario.expect(|ctx| {
        let not_connected = !ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_not_exists = !ctx.server(|s| s.user_exists(&client_key));
        (not_connected && user_not_exists).then_some(())
    });
}

// ============================================================================
// Room Helpers
// ============================================================================

/// Create a new room on the server
pub fn make_room(scenario: &mut Scenario) -> RoomKey {
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.make_room().key()
        })
    })
}

// ============================================================================
// Client Connection Helpers
// ============================================================================

/// Connect a client to the server with default configuration.
/// 
/// This helper:
/// - Starts the client with fast handshake settings
/// - Waits for and processes the auth event
/// - Accepts the connection on the server
/// - Waits for the connect event
/// - Adds the client to the specified room
/// 
/// Returns the ClientKey for the connected client.
pub fn client_connect(
    scenario: &mut Scenario,
    room_key: &RoomKey,
    client_name: &str,
    client_auth: Auth,
    protocol: Protocol,
) -> ClientKey {
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    // Use Real jitter buffer mode (default) - Bypass mode would break tick-based ordering tests
    // client_config.jitter_buffer = JitterBufferType::Bypass;
    client_connect_with_config(scenario, room_key, client_name, client_auth, client_config, protocol)
}

/// Connect a client to the server with a custom ClientConfig.
/// 
/// This is the lower-level helper that `client_connect` uses internally.
/// Use this when you need to customize the client configuration (e.g., for timeout tests).
pub fn client_connect_with_config(
    scenario: &mut Scenario,
    room_key: &RoomKey,
    client_name: &str,
    client_auth: Auth,
    client_config: ClientConfig,
    protocol: Protocol,
) -> ClientKey {
    let client_key = scenario.client_start(client_name, client_auth.clone(), client_config, protocol);

    // Server: read auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) = server.read_event::<AuthEvent<Auth>>() {
                if incoming_client_key == client_key && incoming_auth == client_auth {
                    return Some(incoming_client_key);
                }
            }
            return None;
        })
    });

    // Server: accept connection
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    // Server: read connect event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_client_key) = server.read_event::<ConnectEvent>() {
                if incoming_client_key == client_key {
                    return Some(());
                }
            }
            return None;
        })
    });

    // Server: add client to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room to exist").add_user(&client_key);
        });
    });

    // Verify client has fully established the connection (both client-side and server-side)
    scenario.expect(|ctx| {
        let client_connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        (client_connected && user_exists).then_some(())
    });

    // Allow the next call to be either mutate() or expect()
    scenario.allow_flexible_next();

    client_key
}
