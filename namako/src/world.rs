//! Macro-based World and step definitions for Naia smoke tests.
//!
//! This module defines the `SmokeWorld` type and step implementations
//! using namako's `#[given]`, `#[when]`, `#[then]` macros with inventory.

use namako::{World, given, when, then};

/// Smoke test world - minimal state for proving execution dispatch.
#[derive(Debug, Default, World)]
pub struct SmokeWorld {
    /// Whether a server is running
    server_running: bool,
    /// Number of connected clients
    connected_clients: u32,
}

// ============================================================================
// Step Definitions - these use macros to generate NPAP metadata at compile time
// ============================================================================

/// Step: Given a server is running
#[given("a server is running")]
fn given_server_running(world: &mut SmokeWorld) {
    world.server_running = true;
    world.connected_clients = 0;
}

/// Step: When a client connects
#[when("a client connects")]
fn when_client_connects(world: &mut SmokeWorld) {
    assert!(world.server_running, "Cannot connect: server is not running");
    world.connected_clients += 1;
}

/// Step: Then the server has {int} connected client(s)
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(world: &mut SmokeWorld, expected: usize) {
    assert_eq!(
        world.connected_clients as usize, expected,
        "Expected {} clients but found {}",
        expected, world.connected_clients
    );
}

/// Step: Given a client connects (for scenarios using And/But after Given)
#[given("a client connects")]
fn given_client_connects(world: &mut SmokeWorld) {
    assert!(world.server_running, "Cannot connect: server is not running");
    world.connected_clients += 1;
}

/// Step: When the server disconnects the client
#[when("the server disconnects the client")]
fn when_server_disconnects(world: &mut SmokeWorld) {
    assert!(world.server_running, "Server is not running");
    assert!(world.connected_clients > 0, "No clients to disconnect");
    world.connected_clients -= 1;
}
