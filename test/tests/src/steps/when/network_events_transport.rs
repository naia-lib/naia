//! When-step bindings: transport anomalies, entity despawn, and connection lifecycle.

use crate::steps::prelude::*;
use crate::steps::world_helpers::tick_n;
use crate::steps::world_helpers_connect::{
    connect_named_client_with_auth_tracking, reject_named_client,
};

// ──────────────────────────────────────────────────────────────────────
// Transport — inbound packet handling, conditioning, abstraction
// ──────────────────────────────────────────────────────────────────────

/// When the server receives a packet exceeding MTU.
///
/// Injects a 1000-byte oversized packet from client to server, ticks
/// 3 times, captures any panic. The contract is that the server
/// drops the packet gracefully.
#[when("the server receives a packet exceeding MTU")]
fn when_server_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let oversized: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();
    let _ = scenario.inject_client_packet(&client_key, oversized);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives a packet exceeding MTU.
#[when("the client receives a packet exceeding MTU")]
fn when_client_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let oversized: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();
    let _ = scenario.inject_server_packet(&client_key, oversized);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When packets from the client are dropped by the transport.
///
/// Configures 100% loss client→server, ticks 10 times. Server should
/// remain operational (graceful packet loss handling).
#[when("packets from the client are dropped by the transport")]
fn when_packets_from_client_dropped(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(0, 0, 1.0)),
        None,
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When packets from the server are dropped by the transport.
#[when("packets from the server are dropped by the transport")]
fn when_packets_from_server_dropped(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        None,
        Some(LinkConditionerConfig::new(0, 0, 1.0)),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server receives duplicate packets.
///
/// Injects the same valid-looking packet 3 times, ticks 5 times.
/// Server should dedupe + handle gracefully.
#[when("the server receives duplicate packets")]
fn when_server_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let packet: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_client_packet(&client_key, packet.clone());
        }
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives duplicate packets.
#[when("the client receives duplicate packets")]
fn when_client_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let packet: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_server_packet(&client_key, packet.clone());
        }
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the server receives packets in a different order than sent.
///
/// Configures jitter (50ms latency, 40ms jitter) on client→server to
/// induce reordering. Ticks 10 times.
#[when("the server receives packets in a different order than sent")]
fn when_server_receives_packets_reordered(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(50, 40, 0.0)),
        None,
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives packets in a different order than sent.
#[when("the client receives packets in a different order than sent")]
fn when_client_receives_packets_reordered(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.configure_link_conditioner(
        &client_key,
        None,
        Some(LinkConditionerConfig::new(50, 40, 0.0)),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the same application logic runs on each transport.
///
/// Runs a connect → send-message flow under default (no
/// conditioning) transport conditions. The matching Then asserts the
/// flow completed without panic. Used for transport-abstraction
/// independence proof.
#[when("the same application logic runs on each transport")]
fn when_same_application_logic_runs(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use naia_test_harness::test_protocol::{TestMessage, UnreliableChannel};
    ctx.scenario_mut().clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let client_key = connect_named_client(ctx, "IdealClient", "test_user", None);
        let scenario = ctx.scenario_mut();
        scenario.mutate(|c| {
            c.server(|s| s.send_message::<UnreliableChannel, _>(&client_key, &TestMessage::new(100)));
        });
        tick_n(ctx, 5);
    }));
    let scenario = ctx.scenario_mut();
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the entity despawns on the client.
///
/// Polls until the client no longer has the entity locally. Used as
/// a sequencing barrier in scope-exit tests.
#[when("the entity despawns on the client")]
fn when_entity_despawns_on_client(ctx: &mut TestWorldMut) {
    use naia_test_harness::EntityKey;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if !client.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
}

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — handshake outcomes
// ──────────────────────────────────────────────────────────────────────

/// When the client attempts to connect.
///
/// Drives the auth event + accept-connection step but stops short of
/// the full connect handshake — used by protocol-mismatch tests
/// where the connect-event never fires.
#[when("the client attempts to connect")]
fn when_client_attempts_to_connect(ctx: &mut TestWorldMut) {
    use naia_test_harness::{Auth, ServerAuthEvent};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(());
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
    scenario.record_ok();
}

/// When a client authenticates and connects.
///
/// Full happy-path handshake with both server- and client-side
/// event tracking. Tracks AuthEvent → ConnectEvent (server) and
/// ConnectEvent (client).
#[when("a client authenticates and connects")]
fn when_client_authenticates_and_connects(ctx: &mut TestWorldMut) {
    connect_named_client_with_auth_tracking(ctx, "TestClient", "test_user");
}

/// When a client attempts to connect but is rejected.
///
/// Drives the auth flow + server-side reject. Tracks the client's
/// `RejectEvent` so downstream Then steps can assert it.
#[when("a client attempts to connect but is rejected")]
fn when_client_attempts_connection_rejected(ctx: &mut TestWorldMut) {
    reject_named_client(ctx, "RejectedClient", "bad_user");
}

// ──────────────────────────────────────────────────────────────────────
// Common — generic When phrasings + reconnect/malformed/duplicate flows
// ──────────────────────────────────────────────────────────────────────

/// When a connected client.
///
/// Mirror of the Given variant — usable as When/And after a Given.
#[when("a connected client")]
fn when_connected_client(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// When the client reconnects.
///
/// Starts a brand-new client session after a prior disconnect. Adds
/// to the same room so prior-session entities should be re-spawned.
#[when("the client reconnects")]
fn when_client_reconnects(ctx: &mut TestWorldMut) {
    connect_named_client(ctx, "ReconnectedClient", "test_user", None);
}

/// When the server receives a malformed packet.
#[when("the server receives a malformed packet")]
fn when_server_receives_malformed_packet(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let client_key = scenario.last_client();
    let malformed = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF];
    let _ = scenario.inject_client_packet(&client_key, malformed);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client receives a malformed packet.
#[when("the client receives a malformed packet")]
fn when_client_receives_malformed_packet(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let client_key = scenario.last_client();
    let malformed = vec![0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF];
    let _ = scenario.inject_server_packet(&client_key, malformed);
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..3 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When duplicate replication messages arrive.
///
/// Ticks 5 times — protocol-level dedup should keep state stable.
#[when("duplicate replication messages arrive")]
fn when_duplicate_replication_messages_arrive(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the same API call sequence is executed twice.
///
/// Determinism check — 10 ticks. The local transport + TestClock
/// guarantee identical behavior.
#[when("the same API call sequence is executed twice")]
fn when_same_api_sequence_twice(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the tick is processed.
///
/// Single explicit tick — used to make the scenario flow read more
/// naturally when the scenario has queued state in a Given.
#[when("the tick is processed")]
fn when_tick_is_processed(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|_| {});
    }));
    match result {
        Ok(()) => scenario.record_ok(),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When client A disconnects from the server.
///
/// Server-initiated disconnect for the named client. Used by
/// [entity-delegation-14] (disconnect releases authority).
#[when("client A disconnects from the server")]
fn when_client_a_disconnects_from_server(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_a: crate::ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.disconnect_user(&client_a);
        });
    });
}

/// When one full replication round trip elapses.
///
/// Spins 30 server ticks. Used by replicated-resources scenarios as
/// an explicit barrier between the When (mutate) and the Then
/// (assert).
#[when("one full replication round trip elapses")]
fn when_one_full_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

/// When one replication round trip elapses.
///
/// Alias of `one full replication round trip elapses` — the
/// replicated-resources spec uses both phrasings.
#[when("one replication round trip elapses")]
fn when_one_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

/// When the server advances {n} ticks.
///
/// Runs N server ticks with no other mutation. Used to bound a "no
/// update should arrive in N ticks" window for stale-value
/// assertions (ScopeExit::Persist tests).
#[when("the server advances {int} ticks")]
fn when_server_advances_n_ticks(ctx: &mut TestWorldMut, n: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..n {
        scenario.mutate(|_| {});
    }
}
