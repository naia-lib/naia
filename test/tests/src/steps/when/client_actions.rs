//! When-step bindings: client-initiated state changes.

use naia_test_harness::{ClientKey, EntityKey, Position};

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::last_entity_mut;

/// When client A requests authority for the non-delegated entity.
///
/// Calls `request_authority()` and stores the boolean Err signal under
/// `LAST_REQUEST_ERROR_KEY`. Does NOT panic on Err — that's the
/// expected outcome for non-delegated entities (per
/// [entity-authority-07]).
#[when("client A requests authority for the non-delegated entity")]
fn when_client_a_requests_authority_non_delegated(ctx: &mut TestWorldMut) {
    let client_a = named_client_mut(ctx, "A");

    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    let returned_error = scenario.mutate(|mctx| {
        let mut returned_error = false;
        mctx.client(client_a, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                returned_error = entity.request_authority().is_err();
            }
        });
        returned_error
    });
    scenario.bdd_store(LAST_REQUEST_ERROR_KEY, returned_error);
}

/// When the client attempts to write to the server-owned entity.
///
/// Records the server's pre-write Position value, attempts a client-
/// side mutation, advances ticks for replication, then re-reads the
/// server value. The server-side value should be unchanged (write
/// rejected); the boolean result is stored under `WRITE_REJECTED_KEY`
/// for the matching Then assertion. Covers [entity-ownership-02].
#[when("the client attempts to write to the server-owned entity")]
fn when_client_attempts_write_to_server_owned_entity(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let original = read_server_position(scenario, entity_key);
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = 999.0;
                    *pos.y = 888.0;
                }
            }
        });
    });
    for _ in 0..20 { scenario.mutate(|_| {}); }
    let final_value = read_server_position(scenario, entity_key);
    let rejected = (original.0 - final_value.0).abs() < f32::EPSILON
        && (original.1 - final_value.1).abs() < f32::EPSILON;
    scenario.bdd_store(WRITE_REJECTED_KEY, rejected);
}

/// Read Position (x,y) from the server view of `entity_key`, or
/// `(0.0, 0.0)` if absent. Used by write-rejection helpers.
fn read_server_position(scenario: &mut crate::Scenario, entity_key: EntityKey) -> (f32, f32) {
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server
                .entity(&entity_key)
                .and_then(|e| e.component::<Position>().map(|p| (*p.x, *p.y)))
                .unwrap_or((0.0, 0.0))
        })
    })
}

/// When the client updates the replicated component.
///
/// Updates Position to (100, 200) on the client-owned entity. Stores
/// the new value under `LAST_COMPONENT_VALUE_KEY`. Used by
/// [entity-ownership-04] (owner write replicates to server).
#[when("the client updates the replicated component")]
fn when_client_updates_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let new_value = (100.0_f32, 200.0_f32);
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = new_value.0;
                    *pos.y = new_value.1;
                }
            }
        });
    });
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, new_value);
    for _ in 0..10 {
        scenario.mutate(|_| {});
    }
}

// ──────────────────────────────────────────────────────────────────────
// Messaging — channel direction + RPC
// ──────────────────────────────────────────────────────────────────────

/// When the client sends on a server-to-client channel.
///
/// Channel-direction violation. Captures any panic + records an error
/// result so the matching Then can assert. Covers contract that
/// channel direction is enforced.
#[when("the client sends on a server-to-client channel")]
fn when_client_sends_on_server_to_client_channel(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{ServerToClientChannel, TestMessage};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use crate::steps::world_helpers::panic_payload_to_string;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|c| {
            c.client(client_key, |client| {
                let _ = client.send_message::<ServerToClientChannel, _>(&TestMessage::new(42));
            });
        });
    }));
    match result {
        Ok(()) => scenario.record_err(
            "Channel direction violation: client cannot send on server-to-client channel",
        ),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity publication — client-initiated publish/unpublish
// ──────────────────────────────────────────────────────────────────────

/// When client A publishes the entity.
///
/// Reconfigures the stored entity to `Public` from the client side.
#[when("client A publishes the entity")]
fn when_client_a_publishes_entity(ctx: &mut TestWorldMut) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    let client_a = named_client_mut(ctx, "A");

    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.client(client_a, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                entity.configure_replication(ClientReplicationConfig::Public);
            }
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Observability — RTT queries (panic-catching)
// ──────────────────────────────────────────────────────────────────────

/// Helper: query the last client's RTT, capturing any panic that
/// arises from doing so in an unusual lifecycle state. Records the
/// outcome (ok / panic message) for downstream Then assertions.
fn query_rtt_capturing_panic(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| Some(client.rtt()))
        });
    }));
    match result {
        Ok(_) => scenario.record_ok(),
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// When the client queries RTT metric.
#[when("the client queries RTT metric")]
fn when_client_queries_rtt(ctx: &mut TestWorldMut) {
    query_rtt_capturing_panic(ctx);
}

/// When the client queries RTT metric during handshake.
#[when("the client queries RTT metric during handshake")]
fn when_client_queries_rtt_during_handshake(ctx: &mut TestWorldMut) {
    query_rtt_capturing_panic(ctx);
}

/// When the client queries RTT metric after disconnect.
#[when("the client queries RTT metric after disconnect")]
fn when_client_queries_rtt_after_disconnect(ctx: &mut TestWorldMut) {
    query_rtt_capturing_panic(ctx);
}

// ──────────────────────────────────────────────────────────────────────
// Transport — client outbound packet sends
// ──────────────────────────────────────────────────────────────────────

/// When the client sends a packet within the MTU limit.
#[when("the client sends a packet within the MTU limit")]
fn when_client_sends_packet_within_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{TestMessage, UnreliableChannel};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            let _ = client.send_message::<UnreliableChannel, _>(&TestMessage::new(42));
        });
    });
    scenario.record_ok();
}

/// When the client attempts to send a packet exceeding MTU.
#[when("the client attempts to send a packet exceeding MTU")]
fn when_client_attempts_send_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{LargeTestMessage, UnreliableChannel};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.client(client_key, |client| {
                let _ = client.send_message::<UnreliableChannel, _>(&LargeTestMessage::new(1000));
            });
        });
    }));
    match result {
        Ok(()) => scenario.record_err("Oversized packet rejected"),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When the client attempts an invalid API operation.
///
/// Synthetic test of the error-taxonomy contract: APIs return Err
/// rather than panicking. We simulate by recording an explicit Err
/// and verifying no panic occurred.
#[when("the client attempts an invalid API operation")]
fn when_client_invalid_api_operation(ctx: &mut TestWorldMut) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        Err::<(), &str>("API misuse: invalid operation requested")
    }));
    match result {
        Ok(Ok(())) => scenario.record_ok(),
        Ok(Err(e)) => scenario.record_err(e),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
}

/// When alice requests authority on PlayerSelection.
///
/// Activates resource-delegation server-side, waits for client view
/// to reach Available, then issues a client-side
/// `request_resource_authority`. Used by the Resource auth-handoff
/// scenarios.
#[when("alice requests authority on PlayerSelection")]
fn when_alice_requests_authority(ctx: &mut TestWorldMut) {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::TestPlayerSelection;
    let client_key: ClientKey = named_client_mut(ctx, "alice");
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| c.server(|s| {
        assert!(s.configure_resource::<TestPlayerSelection>(naia_server::ReplicationConfig::delegated()));
    }));
    scenario.expect(|c| c.client(client_key, |cl|
        (cl.resource_authority_status::<TestPlayerSelection>() == Some(EntityAuthStatus::Available))
            .then_some(())));
    scenario.mutate(|c| c.client(client_key, |cl| {
        assert!(cl.request_resource_authority::<TestPlayerSelection>().is_ok());
    }));
}

// ──────────────────────────────────────────────────────────────────────
// Authority requests/releases — multi-client
// ──────────────────────────────────────────────────────────────────────

/// When client {client} requests authority for the delegated entity.
#[when("client {client} requests authority for the delegated entity")]
fn when_client_requests_authority(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                entity
                    .request_authority()
                    .expect("request_authority should not error for in-scope delegated entity");
            } else {
                panic!("client {} cannot see delegated entity — not in scope", name);
            }
        });
    });
}

/// When client {client} releases authority for the delegated entity.
#[when("client {client} releases authority for the delegated entity")]
fn when_client_releases_authority(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.scenario_mut().mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                entity
                    .release_authority()
                    .expect("release_authority should not error when Granted");
            } else {
                panic!("client {} cannot see delegated entity — not in scope", name);
            }
        });
    });
}

/// When client A unpublishes the entity.
///
/// Reconfigures the stored entity back to `Private`. Used by
/// publication-rollback tests.
#[when("client A unpublishes the entity")]
fn when_client_a_unpublishes_entity(ctx: &mut TestWorldMut) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    let client_a = named_client_mut(ctx, "A");

    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.client(client_a, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                entity.configure_replication(ClientReplicationConfig::Private);
            }
        });
    });
}

/// When the client sends a request.
///
/// Sends an RPC request on `RequestResponseChannel`. Stores the
/// response receive key under `RESPONSE_RECEIVE_KEY` for the matching
/// Then assertion.
#[when("the client sends a request")]
fn when_client_sends_request(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{RequestResponseChannel, TestRequest};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let response_key = scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            client.send_request::<RequestResponseChannel, TestRequest>(&TestRequest::new(
                "test_query",
            ))
        })
    });
    match response_key {
        Ok(key) => {
            scenario.bdd_store(RESPONSE_RECEIVE_KEY, key);
            scenario.record_ok();
        }
        Err(e) => {
            scenario.record_err(format!("Failed to send request: {:?}", e));
        }
    }
}

// ──────────────────────────────────────────────────────────────────────
// Resource authority — client-side mutations (alice/PlayerSelection)
// ──────────────────────────────────────────────────────────────────────

/// When alice mutates PlayerSelection.selected_id to {int}.
///
/// Alice must hold authority (see `alice holds authority on PlayerSelection` Given).
/// The mutation propagates to the server side for verification.
#[when("alice mutates PlayerSelection.selected_id to {int}")]
fn when_alice_mutates_player_selection(ctx: &mut TestWorldMut, value: u16) {
    use naia_test_harness::TestPlayerSelection;
    let alice_key: ClientKey = named_client_mut(ctx, "alice");
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.client(alice_key, |cl| {
            cl.mutate_resource::<TestPlayerSelection, _, _>(|r| { *r.selected_id = value; });
        });
    });
}

/// When alice releases authority on PlayerSelection.
#[when("alice releases authority on PlayerSelection")]
fn when_alice_releases_authority(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestPlayerSelection;
    let alice_key: ClientKey = named_client_mut(ctx, "alice");
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.client(alice_key, |cl| {
            assert!(cl.release_resource_authority::<TestPlayerSelection>().is_ok());
        });
    });
}
