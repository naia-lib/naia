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
pub const SECOND_CLIENT_KEY: &str = "second_client";
pub const LAST_COMPONENT_VALUE_KEY: &str = "last_component_value";
pub const WRITE_REJECTED_KEY: &str = "write_rejected";
pub const LAST_REQUEST_ERROR_KEY: &str = "last_request_error";

// Multi-entity tests: A/B labels (priority_accumulator B-BDD-8).
pub const ENTITY_A_KEY: &str = "priority_acc_entity_a";
pub const ENTITY_B_KEY: &str = "priority_acc_entity_b";
pub const SPAWN_BURST_KEYS: &str = "priority_acc_burst_keys";

// Entity-replication tests.
pub const INITIAL_ENTITY_KEY: &str = "initial_entity_key";
pub const CLIENT_LOCAL_VALUE_KEY: &str = "client_local_value";

// Messaging RPC tests.
pub const RESPONSE_RECEIVE_KEY: &str = "response_receive_key";

/// Connect a test client by short label ("A", "B", ...).
///
/// Creates a client with name `"Client {label}"`, auth username
/// `"client_{label_lowercase}"`, and stores the resulting `ClientKey`
/// under `client_key_storage(label)` for downstream lookup.
///
/// Used by multi-client tests (entity-publication, entity-authority,
/// scope-propagation, etc.) where bindings reference clients by
/// label rather than the singleton "last client".
///
/// # Example
/// ```ignore
/// #[given("client {word} connects")]
/// fn given_client_named_connects(ctx: &mut TestWorldMut, name: String) {
///     connect_test_client(ctx, &name);
/// }
/// ```
pub fn connect_test_client(ctx: &mut TestWorldMut, label: &str) -> crate::ClientKey {
    let client_key = connect_named_client(
        ctx,
        &format!("Client {}", label),
        &format!("client_{}", label.to_lowercase()),
        None,
    );
    ctx.scenario_mut()
        .bdd_store(&client_key_storage(label), client_key);
    client_key
}

/// Look up the BDD-stored entity key for a label like "A" or "B".
/// Used by multi-entity tests (priority accumulator B-BDD-8 and
/// future scenarios that work with named entity pairs).
pub fn entity_label_to_key_storage(label: &str) -> &'static str {
    match label {
        "A" => ENTITY_A_KEY,
        "B" => ENTITY_B_KEY,
        other => panic!("unknown entity label '{}' — expected 'A' or 'B'", other),
    }
}

/// BDD-store key for a named client.
///
/// Step bindings that operate on multiple named clients ("client A",
/// "client B") use this to look up the corresponding `ClientKey`.
/// Avoids the per-file `format!("client_{}", name)` duplication.
pub fn client_key_storage(name: &str) -> String {
    format!("client_{}", name)
}

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_test_harness::{
    protocol, Auth, ClientConnectEvent, ServerAuthEvent, ServerConnectEvent, TrackedClientEvent,
    TrackedServerEvent,
};

use crate::TestWorldMut;

/// Idempotently start the server. If the scenario isn't initialized
/// yet, init it, start the server with default config, create a
/// default room, and store it as `last_room`. If it's already
/// initialized, no-op.
///
/// Used by feature files that may be authored either with or without
/// an explicit `Given a server is running` precondition (the
/// replicated-resources scenarios use the implicit form).
pub fn ensure_server_started(ctx: &mut TestWorldMut) {
    use naia_server::ServerConfig;
    if ctx.is_initialized() {
        return;
    }
    let scenario = ctx.init();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|c| c.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);
}

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
    connect_named_client(ctx, "TestClient", "test_user", None);
}

/// Connect a client with an explicit name + username. Optional
/// `extra_setup` runs after the room-add but before the
/// `ClientConnectEvent` wait — typically used by world-integration
/// tests to also include a specific entity in the new client's scope.
///
/// # Example
/// ```ignore
/// connect_named_client(ctx, "SecondClient", "second_client", Some(Box::new(move |scenario, client_key| {
///     scenario.mutate(|m| m.server(|s| {
///         if let Some(mut scope) = s.user_scope_mut(&client_key) {
///             scope.include(&entity_key);
///         }
///     }));
/// })));
/// ```
pub fn connect_named_client(
    ctx: &mut TestWorldMut,
    client_name: &str,
    username: &str,
    extra_setup: Option<Box<dyn FnOnce(&mut crate::Scenario, crate::ClientKey)>>,
) -> crate::ClientKey {
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();

    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;

    let client_key = scenario.client_start(
        client_name,
        Auth::new(username, "password"),
        client_config,
        test_protocol,
    );

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

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });

    if let Some(setup) = extra_setup {
        setup(scenario, client_key);
    }

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientConnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Connect);

    scenario.allow_flexible_next();
    client_key
}
