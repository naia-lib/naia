//! Then-step bindings: observable state predicates.
//!
//! State assertions check the system's *current* observable state —
//! number of connected clients, which entities a client sees, what
//! authority status a client holds, etc. Distinct from
//! [`event_assertions`](super::event_assertions) which assert on
//! the *history* of emitted events.

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::{last_entity_ref, named_client_ref};

/// Then the server has {int} connected client(s).
#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(ctx: &TestWorldRef, expected: usize) {
    let scenario = ctx.scenario();
    let count = scenario.server().expect("server").users_count();
    assert_eq!(
        count, expected,
        "server should have {} connected clients",
        expected
    );
}

// ──────────────────────────────────────────────────────────────────────
// Server-side instrumentation snapshots
// ──────────────────────────────────────────────────────────────────────

/// Then the scope change queue depth is 0.
///
/// Asserts that the server's scope-change queue drained cleanly after
/// the last tick. Used by scope-propagation tests.
#[then("the scope change queue depth is 0")]
fn then_scope_change_queue_depth_is_zero(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let depth = ctx.scenario().scope_change_queue_len();
    if depth == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected scope_change_queue depth 0, got {}",
            depth
        ))
    }
}

/// Then the total dirty update candidate count is 0.
///
/// Asserts that the per-tick dirty-update set drained cleanly. Used
/// by update-candidate-set tests.
#[then("the total dirty update candidate count is 0")]
fn then_total_dirty_update_candidate_count_is_zero(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let count = ctx.scenario().total_dirty_update_count();
    if count == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected total dirty update candidate count 0, got {}",
            count
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Diff-handler receiver-count assertions (immutable-component tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the global diff handler has 0 receivers.
#[then("the global diff handler has 0 receivers")]
fn then_global_diff_handler_has_zero_receivers(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected 0 global diff-handler receivers, got {}",
            snapshot.global_receivers
        ))
    }
}

/// Then the global diff handler has 1 receiver.
#[then("the global diff handler has 1 receiver")]
fn then_global_diff_handler_has_one_receiver(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 1 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Expected 1 global diff-handler receiver, got {}",
            snapshot.global_receivers
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Component-replication assertions (spawn-with-components tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the entity spawns on the client with Position and Velocity.
#[then("the entity spawns on the client with Position and Velocity")]
fn then_entity_spawns_with_position_and_velocity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position, Velocity};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() && entity.has_component::<Velocity>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Component-presence assertions (world-integration tests)
// ──────────────────────────────────────────────────────────────────────

/// Then the client world has the component on the entity.
///
/// Polls until the client's local entity has Position. Covers
/// [world-integration-08.t1].
#[then("the client world has the component on the entity")]
fn then_client_world_has_component_on_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.has_component::<Position>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client world no longer has the component on the entity.
///
/// Polls until the client's local entity no longer has Position.
/// Covers [world-integration-09.t1].
#[then("the client world no longer has the component on the entity")]
fn then_client_world_no_longer_has_component(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.component::<Position>().is_none() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the second client has the entity in its world.
///
/// Covers [world-integration-05.t1] (late-joining client receives
/// current snapshot).
#[then("the second client has the entity in its world")]
fn then_second_client_has_entity_in_world(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let second_client: naia_test_harness::ClientKey = ctx
        .scenario()
        .bdd_get(SECOND_CLIENT_KEY)
        .expect("second client not connected");
    let entity_key = last_entity_ref(ctx);
    ctx.client(second_client, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Authority status
// ──────────────────────────────────────────────────────────────────────

/// Then client A observes no authority status for the entity.
///
/// Covers [entity-authority-01.t1] (authority None for non-delegated).
#[then("client A observes no authority status for the entity")]
fn then_client_a_observes_no_authority_status(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_a = named_client_ref(ctx, "A");
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                None => AssertOutcome::Passed(()),
                Some(status) => AssertOutcome::Failed(format!(
                    "expected None authority for non-delegated entity, got {:?}",
                    status
                )),
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the authority request fails with an error.
///
/// Reads the `LAST_REQUEST_ERROR_KEY` boolean stored by the matching
/// When binding. Covers [entity-authority-07.t1].
#[then("the authority request fails with an error")]
fn then_authority_request_fails_with_error(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    match ctx
        .scenario()
        .bdd_get::<bool>(LAST_REQUEST_ERROR_KEY)
    {
        Some(true) => AssertOutcome::Passed(()),
        Some(false) => AssertOutcome::Failed(
            "expected request_authority to return Err for non-delegated entity, got Ok".to_string(),
        ),
        None => AssertOutcome::Failed("no request result stored".to_string()),
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity ownership
// ──────────────────────────────────────────────────────────────────────

/// Then the entity owner is the client.
#[then("the entity owner is the client")]
fn then_entity_owner_is_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{EntityOwner};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.owner() {
                EntityOwner::Client(_) => AssertOutcome::Passed(()),
                other => AssertOutcome::Failed(format!(
                    "Expected EntityOwner::Client for owned entity, got {:?}",
                    other
                )),
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity owner is the server.
#[then("the entity owner is the server")]
fn then_entity_owner_is_server(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{EntityOwner};
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if entity.owner() == EntityOwner::Server {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Failed(format!(
                    "Expected entity owner to be Server, but was {:?}",
                    entity.owner()
                ))
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the server no longer has the entity.
///
/// Covers [entity-ownership-08.t1] (owner disconnect despawns).
#[then("the server no longer has the entity")]
fn then_server_no_longer_has_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if server.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Then the write is rejected.
///
/// Reads the `WRITE_REJECTED_KEY` boolean set by the matching When
/// binding. Covers [entity-ownership-02].
#[then("the write is rejected")]
fn then_write_is_rejected(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let rejected: bool = ctx
        .scenario()
        .bdd_get(WRITE_REJECTED_KEY)
        .unwrap_or(false);
    if rejected {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(
            "Expected write to be rejected, but server state was modified".to_string(),
        )
    }
}

/// Then the server observes the component update.
///
/// Polls until server-side Position equals the value stored under
/// `LAST_COMPONENT_VALUE_KEY` by the matching When binding.
#[then("the server observes the component update")]
fn then_server_observes_component_update(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use crate::steps::world_helpers_connect::assert_server_position_eq;
    let entity_key = last_entity_ref(ctx);
    let expected: (f32, f32) = ctx.scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    assert_server_position_eq(ctx, entity_key, expected)
}

// ──────────────────────────────────────────────────────────────────────
// Messaging — channel direction
// ──────────────────────────────────────────────────────────────────────

/// Then the send returns an error.
///
/// Reads the recorded operation result from the matching When
/// (e.g. `client sends on a server-to-client channel`) and asserts
/// it is an error (not panic, not Ok).
#[then("the send returns an error")]
fn then_send_returns_error(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        !result.is_ok,
        "Expected send to return error, but it succeeded"
    );
    assert!(
        result.panic_msg.is_none(),
        "Send caused a panic instead of returning an error: {:?}",
        result.panic_msg
    );
}

/// Then the client receives messages A B C in order.
#[then("the client receives messages A B C in order")]
fn then_client_receives_messages_abc_in_order(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let client_key = ctx.last_client();
    let mut received: Vec<u32> = Vec::new();
    ctx.client(client_key, |client| {
        for msg in client.read_message::<OrderedChannel, TestMessage>() {
            received.push(msg.value);
        }
    });
    if received.len() < 3 {
        return AssertOutcome::Pending;
    }
    if received == [1, 2, 3] {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "Messages received out of order. Expected [1, 2, 3] (A, B, C), got {:?}",
            received
        ))
    }
}

/// Then the client's last sequenced message is S3.
///
/// Reads all messages from `SequencedChannel` and asserts the last
/// received value is 3 (S3). SequencedReliable "latest wins" means the
/// channel must not roll back to an older value after S3 is seen.
#[then("the client's last sequenced message is S3")]
fn then_client_last_sequenced_message_is_s3(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::test_protocol::{SequencedChannel, TestMessage};
    let client_key = ctx.last_client();
    let messages: Vec<u32> = ctx.client(client_key, |client| {
        client.read_message::<SequencedChannel, TestMessage>().map(|m| m.value).collect()
    });
    match messages.last().copied() {
        None => AssertOutcome::Pending,
        Some(3) => AssertOutcome::Passed(()),
        Some(v) => AssertOutcome::Failed(format!("Expected last sequenced message S3=3, got {v}")),
    }
}

/// Then the client receives message A exactly once.
#[then("the client receives message A exactly once")]
fn then_client_receives_message_a_exactly_once(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let client_key = ctx.last_client();
    let received: Vec<u32> = ctx.client(client_key, |client| {
        client.read_message::<OrderedChannel, TestMessage>().map(|m| m.value).collect()
    });
    match received.as_slice() {
        [] => AssertOutcome::Pending,
        [1] => AssertOutcome::Passed(()),
        other => AssertOutcome::Failed(format!("Expected [1] (A), got {:?}", other)),
    }
}

/// Then the client receives the response for that request.
#[then("the client receives the response for that request")]
fn then_client_receives_response(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_shared::ResponseReceiveKey;
    use naia_test_harness::test_protocol::TestResponse;
    let client_key = ctx.last_client();
    let scenario = ctx.scenario();
    let response_key: Option<ResponseReceiveKey<TestResponse>> = scenario.bdd_get(RESPONSE_RECEIVE_KEY);
    let Some(response_key) = response_key else {
        return AssertOutcome::Failed(
            "No response receive key was stored - did the client send a request?".to_string(),
        );
    };
    ctx.client(client_key, |client| {
        if client.has_response(&response_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Named-client world assertions (per-user isolation)
// ──────────────────────────────────────────────────────────────────────

/// Then client {client} has the entity in its world.
///
/// Polls until the named client's world contains the last-spawned entity.
/// Use as a prerequisite before asserting another client does NOT have it.
#[then("client {client} has the entity in its world")]
fn then_named_client_has_entity(ctx: &TestWorldRef, name: ClientName) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {client} does not have the entity in its world.
///
/// Polls until the named client's world no longer contains the
/// last-spawned entity (timeout = failure). Covers the case where the
/// entity was previously in scope and a despawn packet is in flight, as
/// well as the case where it was never replicated to this client.
#[then("client {client} does not have the entity in its world")]
fn then_named_client_does_not_have_entity(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}
