//! When-step bindings: client-initiated state changes.

use naia_test_harness::{ClientKey, EntityKey, Position};
use namako_engine::when;

use crate::steps::world_helpers::{
    client_key_storage, LAST_COMPONENT_VALUE_KEY, LAST_ENTITY_KEY, LAST_REQUEST_ERROR_KEY,
    WRITE_REJECTED_KEY,
};
use crate::TestWorldMut;

/// When client A requests authority for the non-delegated entity.
///
/// Calls `request_authority()` and stores the boolean Err signal under
/// `LAST_REQUEST_ERROR_KEY`. Does NOT panic on Err — that's the
/// expected outcome for non-delegated entities (per
/// [entity-authority-07]).
#[when("client A requests authority for the non-delegated entity")]
fn when_client_a_requests_authority_non_delegated(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
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
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    let original_value = scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(entity) = server.entity(&entity_key) {
                if let Some(pos) = entity.component::<Position>() {
                    return (*pos.x, *pos.y);
                }
            }
            (0.0, 0.0)
        })
    });

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

    for _ in 0..20 {
        scenario.mutate(|_| {});
    }

    let final_value = scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(entity) = server.entity(&entity_key) {
                if let Some(pos) = entity.component::<Position>() {
                    return (*pos.x, *pos.y);
                }
            }
            (0.0, 0.0)
        })
    });

    let write_rejected = (original_value.0 - final_value.0).abs() < f32::EPSILON
        && (original_value.1 - final_value.1).abs() < f32::EPSILON;
    scenario.bdd_store(WRITE_REJECTED_KEY, write_rejected);
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
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.client(client_key, |client| {
                let _ = client.send_message::<ServerToClientChannel, _>(&TestMessage::new(42));
            });
        });
    }));
    match result {
        Ok(()) => {
            scenario.record_err(
                "Channel direction violation: client cannot send on server-to-client channel",
            );
        }
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

/// When the client sends a request.
///
/// Sends an RPC request on `RequestResponseChannel`. Stores the
/// response receive key under `RESPONSE_RECEIVE_KEY` for the matching
/// Then assertion.
#[when("the client sends a request")]
fn when_client_sends_request(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{RequestResponseChannel, TestRequest};
    use crate::steps::world_helpers::RESPONSE_RECEIVE_KEY;
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
