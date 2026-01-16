//! Macro-based World and step definitions for Naia smoke tests.
//!
//! This module uses real `naia_test_harness::Scenario` for actual Naia behavior testing.
//! Steps use namako's `#[given]`, `#[when]`, `#[then]` macros with inventory.

use std::time::Duration;

use namako::{World, given, when, then};
use naia_test_harness::{protocol, Auth, Scenario, ClientKey};
use naia_server::{ServerConfig, RoomKey};
use naia_client::{ClientConfig, JitterBufferType};
use naia_test_harness::{ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent};

/// Smoke test world - real Naia behavior using naia_test_harness::Scenario.
#[derive(World)]
pub struct SmokeWorld {
    /// The real naia_test_harness Scenario (optional because we init on first step)
    scenario: Option<Scenario>,
    /// Room key (set when server starts)
    room_key: Option<RoomKey>,
    /// The connected client key (for single-client tests)
    client_key: Option<ClientKey>,
}

impl Default for SmokeWorld {
    fn default() -> Self {
        Self {
            scenario: None,
            room_key: None,
            client_key: None,
        }
    }
}

impl std::fmt::Debug for SmokeWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmokeWorld")
            .field("scenario", &self.scenario.as_ref().map(|_| "Scenario { ... }"))
            .field("room_key", &self.room_key)
            .field("client_key", &self.client_key)
            .finish()
    }
}

// ============================================================================
// Step Definitions - Real Naia behavior via naia_test_harness::Scenario
// ============================================================================

/// Step: Given a server is running
#[given("a server is running")]
fn given_server_running(world: &mut SmokeWorld) {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();
    
    scenario.server_start(ServerConfig::default(), test_protocol);
    
    // Create a room for clients
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    
    world.room_key = Some(room_key);
    world.scenario = Some(scenario);
}

/// Step: When a client connects
#[when("a client connects")]
fn when_client_connects(world: &mut SmokeWorld) {
    let scenario = world.scenario.as_mut().expect("Server must be running first");
    let room_key = world.room_key.as_ref().expect("Room must exist");
    let test_protocol = protocol();
    
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
    
    // Wait for connect event
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
    
    // Add to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(room_key).expect("room exists").add_user(&client_key);
        });
    });
    
    // Verify connection established
    scenario.expect(|ctx| {
        let client_connected = ctx.client(client_key, |c| c.connection_status().is_connected());
        let user_exists = ctx.server(|s| s.user_exists(&client_key));
        (client_connected && user_exists).then_some(())
    });
    
    scenario.allow_flexible_next();
    world.client_key = Some(client_key);
}

/// Step: Then the server has {int} connected client(s)
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(world: &mut SmokeWorld, expected: usize) {
    let scenario = world.scenario.as_mut().expect("Server must be running");
    
    scenario.allow_flexible_next();
    scenario.mutate(|_| {});
    scenario.expect(|ctx| {
        let count = ctx.server(|s| s.users_count());
        (count == expected).then_some(())
    });
}

/// Step: Given a client connects (for scenarios using And/But after Given)
#[given("a client connects")]
fn given_client_connects(world: &mut SmokeWorld) {
    when_client_connects(world);
}

/// Step: When the server disconnects the client
#[when("the server disconnects the client")]
fn when_server_disconnects(world: &mut SmokeWorld) {
    let scenario = world.scenario.as_mut().expect("Server must be running");
    let client_key = world.client_key.expect("Client must exist");
    
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });
    
    // Wait for disconnect to propagate
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(disconnected_key) = server.read_event::<ServerDisconnectEvent>() {
                if disconnected_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    
    scenario.allow_flexible_next();
}

// ============================================================================
// DEMO: Intentional Failure Step (for P0-A Runtime Failure Demo)
// ============================================================================

/// Step: Then the system intentionally fails
/// This step always panics to demonstrate runtime failure handling.
#[then("the system intentionally fails")]
fn then_system_intentionally_fails(_world: &mut SmokeWorld) {
    panic!("INTENTIONAL FAILURE: This step is designed to fail for demo purposes");
}
