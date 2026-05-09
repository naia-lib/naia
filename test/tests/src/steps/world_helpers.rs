//! Shared low-level helpers used by step bindings.
//!
//! **Purpose:** absorb the repeated mutate/expect/until boilerplate so
//! a typical binding becomes ≤ 6 LOC instead of the current 18 LOC
//! median.
//!
//! High-level connect-handshake and entity-spawn helpers live in
//! [`world_helpers_connect`](super::world_helpers_connect).
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

// Tick-buffer tests (messaging-13/14).
pub const TICK_BUFFER_TICK_KEY: &str = "tick_buffer_tick";
pub const TICK_BUFFER_COUNT_KEY: &str = "tick_buffer_count";
pub const TICK_BUFFER_REJECTED_KEY: &str = "tick_buffer_rejected";

// EntityProperty buffering tests (messaging-18/20).
pub const ENTITY_COMMAND_COUNT_KEY: &str = "entity_command_count";

// Room-migration tests (D-A4).
pub const SECOND_ROOM_KEY: &str = "second_room_key";

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

use naia_test_harness::{TrackedClientEvent, TrackedServerEvent};

use crate::{TestWorldMut, TestWorldRef};

// ──────────────────────────────────────────────────────────────────────
// Lookup helpers — entity/client retrieval with descriptive panics
// ──────────────────────────────────────────────────────────────────────
//
// These reduce the most-repeated boilerplate in step bindings. Before
// (50+ call sites):
//
//     let entity_key: EntityKey = scenario
//         .bdd_get(LAST_ENTITY_KEY)
//         .expect("No entity has been created");
//
// After (one line):
//
//     let entity_key = last_entity_mut(ctx);
//
// The mut/ref split mirrors the harness's `Scenario` access pattern:
// Given/When bindings receive `&mut TestWorldMut` (use `_mut`
// variants) and Then bindings receive `&TestWorldRef` (use `_ref`
// variants). The two are split rather than overloaded because
// `TestWorldMut` exposes `scenario_mut()` while `TestWorldRef`
// exposes `scenario()`.

/// Look up the BDD-stored "last entity" from a Given/When context.
///
/// Panics with a descriptive message if no entity has been created
/// (typically a missing precondition Given).
///
/// # Example
/// ```ignore
/// #[when("the server inserts the replicated component")]
/// fn when_server_inserts(ctx: &mut TestWorldMut) {
///     let entity_key = last_entity_mut(ctx);
///     ctx.scenario_mut().mutate(|m| {
///         m.server(|s| { /* mutate s.entity_mut(&entity_key) */ });
///     });
/// }
/// ```
pub fn last_entity_mut(ctx: &mut TestWorldMut) -> naia_test_harness::EntityKey {
    ctx.scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created — missing precondition Given")
}

/// Look up the BDD-stored "last entity" from a Then context.
pub fn last_entity_ref(ctx: &TestWorldRef) -> naia_test_harness::EntityKey {
    ctx.scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created — missing precondition Given")
}

/// Look up a labeled client (e.g. `"A"`, `"B"`, `"alice"`) from a
/// Given/When context. Panics if the client wasn't previously
/// connected via `Given client {label} connects` or similar.
pub fn named_client_mut(ctx: &mut TestWorldMut, label: &str) -> crate::ClientKey {
    ctx.scenario_mut()
        .bdd_get(&client_key_storage(label))
        .unwrap_or_else(|| panic!("client {:?} has not been connected", label))
}

/// Look up a labeled client from a Then context.
pub fn named_client_ref(ctx: &TestWorldRef, label: &str) -> crate::ClientKey {
    ctx.scenario()
        .bdd_get(&client_key_storage(label))
        .unwrap_or_else(|| panic!("client {:?} has not been connected", label))
}

// ──────────────────────────────────────────────────────────────────────
// Tick helpers
// ──────────────────────────────────────────────────────────────────────

/// Advance the scenario `n` ticks with no other mutation.
///
/// Replaces the 25× `for _ in 0..N { scenario.mutate(|_| {}); }`
/// pattern. Names the intent (`tick_n`) instead of inlining the
/// loop, which makes the Given/When bindings noticeably more
/// readable.
///
/// # Example
/// ```ignore
/// #[when("{int} ticks elapse")]
/// fn when_n_ticks_elapse(ctx: &mut TestWorldMut, n: u32) {
///     tick_n(ctx, n);
/// }
/// ```
pub fn tick_n(ctx: &mut TestWorldMut, n: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..n {
        scenario.mutate(|_| {});
    }
}

/// Convert a panic payload into a string message. Used by
/// transport/observability bindings that wrap operations in
/// `catch_unwind`. The `scenario.record_panic(...)` API takes a
/// `String`, but `catch_unwind` returns `Box<dyn Any + Send>` — this
/// helper bridges the two.
pub fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Client-initiated graceful disconnect of the most-recently-connected client.
///
/// The client sends token-authenticated disconnect packets, then the server
/// verifies the token and processes the disconnect. Both sides' disconnect
/// events are tracked for downstream ordering assertions.
pub fn graceful_disconnect_last_client(ctx: &mut TestWorldMut) {
    use naia_test_harness::ClientDisconnectEvent;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Client initiates disconnect — sends token-authenticated disconnect packets
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.disconnect();
        });
    });

    // Wait for the server to process the verified disconnect (user gone)
    scenario.expect(|ctx| {
        let client_disconnected = ctx.client(client_key, |c| {
            c.read_event::<ClientDisconnectEvent>()
        });
        let server_processed = !ctx.server(|s| s.user_exists(&client_key));
        (client_disconnected.is_some() && server_processed).then_some(())
    });

    scenario.track_server_event(TrackedServerEvent::Disconnect);
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);
    scenario.allow_flexible_next();
}

/// Server-initiated disconnect of the most-recently-connected client.
/// Tracks both the server-side and client-side disconnect events so
/// downstream Then steps can assert on them.
pub fn disconnect_last_client(ctx: &mut TestWorldMut) {
    use naia_test_harness::ClientDisconnectEvent;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });
    scenario.track_server_event(TrackedServerEvent::Disconnect);
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientDisconnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);
    scenario.allow_flexible_next();
}
