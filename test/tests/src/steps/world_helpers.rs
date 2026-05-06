//! Shared imperative helpers used by step bindings.
//!
//! **Purpose:** absorb the repeated mutate/expect/until boilerplate so
//! a typical binding becomes ≤ 6 LOC instead of the current 18 LOC
//! median.
//!
//! ## Helper catalog
//!
//! ### Connection
//! - [`connect_client`] — full client-connect handshake (queue auth →
//!   accept → enter room → wait for both server- and client-side
//!   connect events). Used by `a client connects` (Given/When) and
//!   `client {client} connects` etc.
//!
//! ### Coming in Phase B
//! - `tick_once`, `tick_until`, `with_server`, `with_client`,
//!   `expect_server_event`, `expect_client_event`, `store_entity`,
//!   `lookup_entity`, `lookup_client_key`
//!
//! Each helper carries a doc-comment with a usage example. A helper
//! belongs here when it is reusable across ≥ 2 step bindings AND its
//! body is more than a single library call.

// ──────────────────────────────────────────────────────────────────────
// BDD-store keys
// ──────────────────────────────────────────────────────────────────────
//
// Step bindings communicate state across phases via the scenario's
// `bdd_store(key, val)` / `bdd_get(key)` API. Constants here are the
// canonical key strings — using a shared symbol prevents two bindings
// from disagreeing on what `"last_entity"` means.

pub const LAST_ENTITY_KEY: &str = "last_entity";
pub const SPAWN_POSITION_VALUE_KEY: &str = "spawn_position_value";
pub const SPAWN_VELOCITY_VALUE_KEY: &str = "spawn_velocity_value";

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ServerAuthEvent, ServerConnectEvent, TrackedClientEvent,
    TrackedServerEvent,
};

use crate::TestWorldMut;

/// Run the standard client-connect handshake for the next test client.
///
/// The handshake is identical for every test that needs a connected
/// client: build a `ClientConfig` with bypass jitter buffer + zero
/// handshake interval, queue auth, accept on server, add to the
/// scenario's `last_room`, then wait for both server-side
/// `ServerConnectEvent` and client-side `ClientConnectEvent` and
/// track them. The bound step binding becomes a one-liner over this.
///
/// # Example
/// ```ignore
/// #[given("a client connects")]
/// fn given_client_connects(ctx: &mut TestWorldMut) {
///     connect_client(ctx);
/// }
/// ```
pub fn connect_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        "TestClient",
        Auth::new("test_user", "password"),
        client_config,
        test_protocol,
    );

    // Wait for server-side auth, then accept
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

    // Wait for server connect event
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
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    // Wait for client connect event
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
}
