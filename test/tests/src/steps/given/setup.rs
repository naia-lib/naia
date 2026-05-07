//! Given-step bindings: protocol, server, client, room initialization.
//!
//! This module owns the preconditions that put the system into a
//! "ready to act on" state: a running server, one or more connected
//! clients, joined rooms.

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::tick_n;

/// Given a server is running.
///
/// Initializes the scenario, starts a server with default config, and
/// creates a default room (stored as `last_room`). After this step
/// the test world has exactly one server, zero clients, one room.
///
/// **Idempotent.** Safe to use as a feature-file `Background:` step
/// AND as an explicit Given inside a Scenario — the second call
/// no-ops via `ensure_server_started` rather than re-initializing.
/// This matters in Phase C onward, where every grouped feature
/// hoists this Given to a Background block.
#[given("a server is running")]
fn given_server_running(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
}

/// Given a client connects.
///
/// Mirror of the When variant — usable as `Given` (precondition) or
/// `And` after another Given. Drives the standard handshake via
/// [`connect_client`].
#[given("a client connects")]
fn given_client_connects(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// Given client {client} connects.
///
/// Connects a labeled client ("A", "B", ...) and stores the
/// resulting ClientKey under `client_key_storage(name)`. Used by
/// multi-client tests where bindings reference specific clients.
#[given("client {client} connects")]
fn given_client_named_connects(ctx: &mut TestWorldMut, name: ClientName) {
    connect_test_client(ctx, name.as_ref());
}

/// Given a server and one connected client.
///
/// Composite Given used by the replicated-resources scenarios:
/// idempotently start the server (so the scenario can omit a separate
/// `Given a server is running`), then connect a labeled "alice"
/// client. The result is the canonical 1-server-1-client baseline.
#[given("a server and one connected client")]
fn given_server_and_one_client(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
    let _ = connect_test_client(ctx, "alice");
}

/// Given the initial replication round trip has elapsed.
///
/// Spins 20 server ticks to let initial state replicate to all
/// connected clients. Used by replicated-resources scenarios as a
/// barrier between setup and action.
#[given("the initial replication round trip has elapsed")]
fn given_initial_round_trip_elapsed(ctx: &mut TestWorldMut) {
    tick_n(ctx, 20);
}

// ──────────────────────────────────────────────────────────────────────
// Observability — alternative connection lifecycle states
// ──────────────────────────────────────────────────────────────────────

/// Given a client is created but not connected.
///
/// Initiates handshake but does not complete it. Used to test RTT
/// query semantics in the "before fully connected" state.
#[given("a client is created but not connected")]
fn given_client_created_not_connected(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{protocol, Auth};
    let scenario = ctx.scenario_mut();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let _ = scenario.client_start(
        "UnconnectedClient",
        Auth::new("test_user", "password"),
        client_config,
        protocol(),
    );
    scenario.clear_operation_result();
    scenario.record_ok();
}

/// Given a client begins connecting.
///
/// Initiates handshake and ticks once but doesn't complete connection.
/// Used to test RTT during handshake phase.
#[given("a client begins connecting")]
fn given_client_begins_connecting(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{protocol, Auth};
    let scenario = ctx.scenario_mut();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let _ = scenario.client_start(
        "ConnectingClient",
        Auth::new("test_user", "password"),
        client_config,
        protocol(),
    );
    scenario.mutate(|_| {});
    scenario.clear_operation_result();
    scenario.record_ok();
}

/// Given a client connects with latency {n}ms.
///
/// Standard handshake + a `LinkConditionerConfig` of `(latency_ms, 0,
/// 0.0)` applied symmetrically. Used to test RTT convergence under
/// known link characteristics.
#[given("a client connects with latency {int}ms")]
fn given_client_connects_with_latency(ctx: &mut TestWorldMut, latency_ms: u32) {
    connect_client_with_latency(ctx, "LatencyClient", latency_ms);
}

/// Given the client disconnects.
#[given("the client disconnects")]
fn given_client_disconnects(ctx: &mut TestWorldMut) {
    disconnect_last_client(ctx);
}

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — auth + protocol-version setup
// ──────────────────────────────────────────────────────────────────────

/// Given a server is running with auth required.
///
/// Same as `a server is running` but explicitly clears event history
/// so connection-lifecycle scenarios start with a clean slate.
#[given("a server is running with auth required")]
fn given_server_running_with_auth(ctx: &mut TestWorldMut) {
    use naia_server::ServerConfig;
    use naia_test_harness::protocol;
    let scenario = ctx.init();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|c| c.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);
    scenario.clear_event_history();
}

/// Given a server with protocol version {word}.
///
/// Maps version "A"→1 and "B"→2 to a `ProtocolId`. Used by the
/// protocol-mismatch rejection tests.
#[given("a server with protocol version {word}")]
fn given_server_with_protocol_version(ctx: &mut TestWorldMut, version: String) {
    use naia_server::ServerConfig;
    use naia_test_harness::{protocol, ProtocolId};
    let scenario = ctx.init();
    let protocol_id = match version.as_str() {
        "A" => ProtocolId::new(1),
        "B" => ProtocolId::new(2),
        _ => panic!("Unknown protocol version: {}", version),
    };
    scenario.server_start_with_protocol_id(ServerConfig::default(), protocol(), protocol_id);
    scenario.record_ok();
}

/// Given a client with protocol version {word}.
///
/// Same version mapping as the server variant; used to set up
/// matching/mismatching protocol-id pairs.
#[given("a client with protocol version {word}")]
fn given_client_with_protocol_version(ctx: &mut TestWorldMut, version: String) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{protocol, Auth, ProtocolId};
    let scenario = ctx.scenario_mut();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let protocol_id = match version.as_str() {
        "A" => ProtocolId::new(1),
        "B" => ProtocolId::new(2),
        _ => panic!("Unknown protocol version: {}", version),
    };
    scenario.client_start_with_protocol_id(
        "TestClient",
        Auth::new("test_user", "password"),
        client_config,
        protocol(),
        protocol_id,
    );
    scenario.record_ok();
}

// ──────────────────────────────────────────────────────────────────────
// Common — generic scenario aliases
// ──────────────────────────────────────────────────────────────────────

/// Given a test scenario.
///
/// Alias for `a server is running` — the common-feature scenarios use
/// this looser phrasing. Idempotent (`init` is a no-op if already
/// initialized).
#[given("a test scenario")]
fn given_test_scenario(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
}

/// Given a connected client.
#[given("a connected client")]
fn given_connected_client(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// Given a client that was previously connected.
///
/// Same as `a connected client` — phrasing distinct so the
/// reconnection-scenario flow reads naturally.
#[given("a client that was previously connected")]
fn given_client_previously_connected(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// Given a test scenario with deterministic time.
///
/// The harness already uses `TestClock` for deterministic time —
/// this Given just initializes the standard scenario. Phrased
/// distinctly so determinism scenarios self-document.
#[given("a test scenario with deterministic time")]
fn given_test_scenario_deterministic_time(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
}

/// Given a deterministic network input sequence.
///
/// Local transport is deterministic by design. This Given connects a
/// client to establish a baseline state for the subsequent When.
#[given("a deterministic network input sequence")]
fn given_deterministic_network_input(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// Given multiple transport adapters with different quality characteristics.
///
/// Sets up the scenario with a server + room, ready for transport
/// abstraction-independence testing. Different transport qualities
/// are simulated via `LinkConditionerConfig` in the matching When step.
#[given("multiple transport adapters with different quality characteristics")]
fn given_multiple_transport_adapters(ctx: &mut TestWorldMut) {
    use naia_server::ServerConfig;
    use naia_test_harness::protocol;
    let scenario = ctx.init();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|c| c.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);
    scenario.clear_operation_result();
    scenario.record_ok();
}
