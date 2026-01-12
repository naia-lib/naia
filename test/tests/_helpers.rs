use naia_client::{ClientConfig, JitterBufferType};
use naia_server::RoomKey;
use naia_shared::Protocol;
use naia_test::{Auth, ClientKey, ExpectCtx, Scenario, ServerAuthEvent, ServerConnectEvent};
use std::time::Duration;

// ============================================================================
// Test Configuration Helpers
// ============================================================================

/// Create a ClientConfig configured for E2E tests.
///
/// Sets:
/// - `send_handshake_interval = 0` for immediate handshake
/// - `jitter_buffer = Bypass` for immediate packet processing
///
/// Use this instead of `ClientConfig::default()` in all tests.
pub fn test_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

// ============================================================================
// Connection Assertion Helpers
// ============================================================================

/// Assert that a client is connected (both client-side and server-side).
///
/// This helper is meant to be called from within an `expect()` block.
/// Returns `Some(())` if the client is connected, `None` otherwise.
///
/// # Example
/// ```rust,ignore
/// scenario.expect(|ctx| {
///     expect_client_connected(ctx, client_key)
/// });
/// ```
#[allow(dead_code)]
pub fn server_and_client_connected(ctx: &mut ExpectCtx<'_>, client_key: ClientKey) -> Option<()> {
    let connected = ctx.client(client_key, |c| c.connection_status().is_connected());
    let user_exists = ctx.server(|s| s.user_exists(&client_key));
    (connected && user_exists).then_some(())
}

/// Assert that a client is NOT connected.
///
/// This helper is meant to be called from within an `expect()` block.
/// Returns `Some(())` if the client is disconnected, `None` otherwise.
///
/// # Example
/// ```rust,ignore
/// scenario.expect(|ctx| {
///     expect_client_disconnected(ctx, client_key)
/// });
/// ```
#[allow(dead_code)]
pub fn server_and_client_disconnected(
    ctx: &mut ExpectCtx<'_>,
    client_key: ClientKey,
) -> Option<()> {
    let not_connected = !ctx.client(client_key, |c| c.connection_status().is_connected());
    let user_not_exists = !ctx.server(|s| s.user_exists(&client_key));
    (not_connected && user_not_exists).then_some(())
}

// ============================================================================
// Client Connection Helpers
// ============================================================================

/// Connect a client to the server with a custom ClientConfig.
///
/// This helper:
/// - Starts the client with the provided configuration
/// - Waits for and processes the auth event
/// - Accepts the connection on the server
/// - Waits for the connect event
/// - Adds the client to the specified room
///
/// Note: `send_handshake_interval` is always set to 0 for immediate handshake in tests.
///
/// Returns the ClientKey for the connected client.
pub fn client_connect(
    scenario: &mut Scenario,
    room_key: &RoomKey,
    client_name: &str,
    client_auth: Auth,
    client_config: ClientConfig,
    protocol: Protocol,
) -> ClientKey {
    // Allow this to be called after either mutate() or expect()
    scenario.allow_flexible_next();

    // Merge user config with test-required settings
    let mut config = client_config;
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(client_name, client_auth.clone(), config, protocol);

    // Server: read auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) =
                server.read_event::<ServerAuthEvent<Auth>>()
            {
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
            if let Some(incoming_client_key) = server.read_event::<ServerConnectEvent>() {
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
            server
                .room_mut(&room_key)
                .expect("room to exist")
                .add_user(&client_key);
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
