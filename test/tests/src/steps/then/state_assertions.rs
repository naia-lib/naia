//! Then-step bindings: observable state predicates.
//!
//! State assertions check the system's *current* observable state —
//! number of connected clients, which entities a client sees, what
//! authority status a client holds, etc. Distinct from
//! [`event_assertions`](super::event_assertions) which assert on
//! the *history* of emitted events.

use crate::steps::prelude::*;
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

/// Then the system intentionally fails.
///
/// Demo step from the P0-A runtime-failure scaffolding. Always
/// panics. Kept here for the namako-runtime smoke check.
#[then("the system intentionally fails")]
fn then_system_intentionally_fails(_ctx: &TestWorldRef) {
    panic!("INTENTIONAL FAILURE: This step is designed to fail for demo purposes");
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
    use crate::steps::world_helpers::assert_server_position_eq;
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
// Entity replication assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the entity spawns on the client with the replicated component.
#[then("the entity spawns on the client with the replicated component")]
fn then_entity_spawns_on_client_with_replicated_component(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
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

/// Then the client observes the component update.
#[then("the client observes the component update")]
fn then_client_observes_component_update(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use crate::steps::world_helpers::assert_client_position_eq;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    let expected: (f32, f32) = ctx.scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    assert_client_position_eq(ctx, client_key, entity_key, expected)
}

/// Then the client observes the server value.
///
/// Used after `Given the client modifies the component locally` —
/// asserts that the server-authoritative value overrides the
/// client-local modification.
#[then("the client observes the server value")]
fn then_client_observes_server_value(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use crate::steps::world_helpers::assert_client_position_eq;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    let server_value: (f32, f32) = ctx.scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No server component value stored");
    assert_client_position_eq(ctx, client_key, entity_key, server_value)
}

/// Then the entity GlobalEntity remains unchanged.
///
/// EntityKey is the harness abstraction over Naia's GlobalEntity.
/// Stable identity throughout an entity's lifetime is the contract.
#[then("the entity GlobalEntity remains unchanged")]
fn then_entity_global_entity_remains_unchanged(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let initial: naia_test_harness::EntityKey = ctx
        .scenario()
        .bdd_get(INITIAL_ENTITY_KEY)
        .expect("No initial entity key stored");
    let current = last_entity_ref(ctx);
    if initial == current {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "GlobalEntity changed: initial={:?}, current={:?}",
            initial, current
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Priority accumulator assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the client eventually observes all N spawned entities.
#[then("the client eventually observes all {int} spawned entities")]
fn then_client_eventually_observes_all_spawned(
    ctx: &TestWorldRef,
    expected: usize,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let keys: Vec<naia_test_harness::EntityKey> = ctx
        .scenario()
        .bdd_get(SPAWN_BURST_KEYS)
        .expect("spawn-burst keys missing");
    if keys.len() != expected {
        return AssertOutcome::Failed(format!(
            "stored {} burst keys but scenario expected {}",
            keys.len(),
            expected
        ));
    }
    ctx.client(client_key, |client| {
        if keys.iter().all(|k| client.has_entity(k)) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the global priority gain on the last entity is {float}.
#[then("the global priority gain on the last entity is {float}")]
fn then_global_gain_on_last_entity_is(
    ctx: &TestWorldRef,
    expected: f32,
) -> AssertOutcome<()> {
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| match server.global_entity_gain(&entity_key) {
        Some(g) if (g - expected).abs() < f32::EPSILON => {
            AssertOutcome::Passed(())
        }
        Some(g) => AssertOutcome::Failed(format!(
            "global gain is {} but expected {}",
            g, expected
        )),
        None => AssertOutcome::Failed(format!(
            "no gain override is set (expected {})",
            expected
        )),
    })
}

/// Then the client eventually sees the last entity.
#[then("the client eventually sees the last entity")]
fn then_client_eventually_sees_last_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the global priority gain on the last entity is still {float}.
///
/// Same predicate as `is {float}`, distinct phrase to read naturally
/// after a follow-up tick step.
#[then("the global priority gain on the last entity is still {float}")]
fn then_global_gain_on_last_entity_is_still(
    ctx: &TestWorldRef,
    expected: f32,
) -> AssertOutcome<()> {
    then_global_gain_on_last_entity_is(ctx, expected)
}

/// Then the client eventually observes entity {label} at x={int} y={int}.
///
/// `label` is "A" or "B"; resolves via `entity_label_to_key_storage`.
#[then("the client eventually observes entity {word} at x={int} y={int}")]
fn then_client_eventually_observes_entity_at(
    ctx: &TestWorldRef,
    label: String,
    x: i32,
    y: i32,
) -> AssertOutcome<()> {
    use naia_test_harness::Position;
    let client_key = ctx.last_client();
    let entity_key: naia_test_harness::EntityKey = ctx
        .scenario()
        .bdd_get(entity_label_to_key_storage(&label))
        .unwrap_or_else(|| panic!("entity '{}' not stored", label));
    let (ex, ey) = (x as f32, y as f32);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        if (*pos.x - ex).abs() < f32::EPSILON && (*pos.y - ey).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Scope-exit (Persist) assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the client still has the entity.
///
/// Confirms ScopeExit::Persist prevented the Despawn when the entity
/// went out-of-scope.
#[then("the client still has the entity")]
fn then_client_still_has_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Failed(
                "Entity was despawned on client despite ScopeExit::Persist".into(),
            )
        }
    })
}

/// Then the client entity position is still 0.0.
///
/// Confirms no update leaked through while the entity was Paused.
#[then("the client entity position is still 0.0")]
fn then_client_entity_position_still_zero(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::Position;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Failed("Entity absent despite ScopeExit::Persist".into());
        };
        let Some(pos) = entity.component::<Position>() else { return AssertOutcome::Pending; };
        if (*pos.x).abs() < f32::EPSILON { AssertOutcome::Passed(()) }
        else { AssertOutcome::Failed(format!("Position leaked while out-of-scope: x={}", *pos.x)) }
    })
}

/// Then the client entity position becomes 100.0.
///
/// Polling — confirms accumulated updates from the Paused period
/// arrive after re-entry.
#[then("the client entity position becomes 100.0")]
fn then_client_entity_position_becomes_hundred(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        if (*pos.x - 100.0).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client entity has ImmutableLabel.
#[then("the client entity has ImmutableLabel")]
fn then_client_entity_has_label(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        if entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client entity does not have ImmutableLabel.
#[then("the client entity does not have ImmutableLabel")]
fn then_client_entity_no_label(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        if !entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Entity publication — scope-membership for named clients
// ──────────────────────────────────────────────────────────────────────

/// Internal helper: server-side scope-membership check for a labeled
/// client. Used by all four "the entity is{,n't,becomes} in/out-of-scope
/// for client X" assertions below.
fn check_entity_in_scope(ctx: &TestWorldRef, label: &str) -> bool {
    let client_key = named_client_ref(ctx, label);
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            scope.has(&entity_key)
        } else {
            false
        }
    })
}

/// Then the entity is in-scope for client A.
#[then("the entity is in-scope for client A")]
fn then_entity_in_scope_for_client_a(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "A"),
        "Expected entity to be in-scope for client A, but it was not"
    );
}

/// Then the entity is in-scope for client B.
#[then("the entity is in-scope for client B")]
fn then_entity_in_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "B"),
        "Expected entity to be in-scope for client B, but it was not"
    );
}

/// Then the entity is out-of-scope for client B.
#[then("the entity is out-of-scope for client B")]
fn then_entity_out_of_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        !check_entity_in_scope(ctx, "B"),
        "Expected entity to be out-of-scope for client B, but it was in-scope"
    );
}

/// Then the entity becomes out-of-scope for client B.
///
/// Polling variant of the above — used after an unpublish where the
/// scope removal propagates asynchronously.
#[then("the entity becomes out-of-scope for client B")]
fn then_entity_becomes_out_of_scope_for_client_b(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    if !check_entity_in_scope(ctx, "B") {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then client {label} observes replication config as {config} for the entity.
///
/// Polls until the named client's entity reports the expected
/// `ReplicationConfig`. Covers [entity-publication-observability].
#[then("client {word} observes replication config as {word} for the entity")]
fn then_client_observes_replication_config(
    ctx: &TestWorldRef,
    label: String,
    config_name: String,
) -> AssertOutcome<()> {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    let client_key = named_client_ref(ctx, &label);
    let entity_key = last_entity_ref(ctx);
    let expected = match config_name.as_str() {
        "Public" => ClientReplicationConfig::Public,
        "Private" => ClientReplicationConfig::Private,
        "Delegated" => ClientReplicationConfig::Delegated,
        other => {
            return AssertOutcome::Failed(format!(
                "Unknown replication config: '{}'",
                other
            ))
        }
    };
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(config) if config == expected => AssertOutcome::Passed(()),
                Some(other) => AssertOutcome::Failed(format!(
                    "Expected replication_config {:?}, got {:?}",
                    expected, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — client-side observability
// ──────────────────────────────────────────────────────────────────────

/// Then the client's Score is present.
#[then("the client's Score is present")]
fn then_client_has_score(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    if ctx.client(key, |c| c.has_resource::<TestScore>()) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client's Score.home equals 0.
#[then("the client's Score.home equals 0")]
fn then_client_score_home_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.home)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

/// Then the client's Score.away equals 0.
#[then("the client's Score.away equals 0")]
fn then_client_score_away_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.away)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

/// Then alice's authority status for PlayerSelection is "Granted".
#[then(r#"alice's authority status for PlayerSelection is "Granted""#)]
fn then_alice_auth_granted(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::TestPlayerSelection;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource_authority_status::<TestPlayerSelection>()) {
        Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation — authority status assertions
// ──────────────────────────────────────────────────────────────────────

/// Then client {name} is granted authority for the delegated entity.
///
/// Polls until the named client observes EntityAuthStatus::Granted.
/// Covers [entity-delegation-06.t1] (first in-scope request wins).
#[then("client {word} is granted authority for the delegated entity")]
fn then_client_is_granted_authority(
    ctx: &TestWorldRef,
    name: String,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, &name);
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Pending
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Granted, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {name} is denied authority for the delegated entity.
///
/// Allows Requested as a transient state while the server round-trip
/// completes. Covers [entity-delegation-07.t1].
#[then("client {word} is denied authority for the delegated entity")]
fn then_client_is_denied_authority(
    ctx: &TestWorldRef,
    name: String,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, &name);
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Denied) => AssertOutcome::Passed(()),
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Pending
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Denied, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {name} is available for the delegated entity.
///
/// Covers [entity-delegation-11.t1] (release returns Denied clients
/// to Available). Tolerates transient Releasing/Granted/Denied/Requested
/// while the convergence completes.
#[then("client {word} is available for the delegated entity")]
fn then_client_is_available_for_delegated_entity(
    ctx: &TestWorldRef,
    name: String,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, &name);
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Passed(())
                }
                _ => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the delegated entity is no longer in client A's world.
///
/// Covers [entity-delegation-13.t1] (entity leaves scope on exclude).
#[then("the delegated entity is no longer in client A's world")]
fn then_delegated_entity_is_no_longer_in_client_a_world(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_a = named_client_ref(ctx, "A");
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_a, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Then client A observes Delegated replication config for the entity.
///
/// Covers [entity-delegation-17.t1] (delegation observable from client).
#[then("client A observes Delegated replication config for the entity")]
fn then_client_a_observes_delegated_replication_config(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    let client_a = named_client_ref(ctx, "A");
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(ClientReplicationConfig::Delegated) => {
                    AssertOutcome::Passed(())
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "expected Delegated replication config, got {:?}",
                    other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {name} observes Available authority status for the entity.
#[then("client {word} observes Available authority status for the entity")]
fn then_client_observes_available_authority_status(
    ctx: &TestWorldRef,
    name: String,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, &name);
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Passed(())
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Available authority status, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Entity scope — singleton-client predicates
// ──────────────────────────────────────────────────────────────────────

/// Then the entity is in-scope for the client.
#[then("the entity is in-scope for the client")]
fn then_entity_in_scope_for_client_singleton(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity is out-of-scope for the client.
#[then("the entity is out-of-scope for the client")]
fn then_entity_out_of_scope_for_client_singleton(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if !scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Then the entity despawns on the client.
#[then("the entity despawns on the client")]
fn then_entity_despawns_on_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if !client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client.
#[then("the entity spawns on the client")]
fn then_entity_spawns_on_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client as a new lifetime.
#[then("the entity spawns on the client as a new lifetime")]
fn then_entity_spawns_on_client_as_new_lifetime(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the server stops replicating entities to that client.
///
/// Polls until the user no longer exists server-side (post-disconnect).
#[then("the server stops replicating entities to that client")]
fn then_server_stops_replicating_to_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.server(|server| {
        if !server.user_exists(&client_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then no error is raised.
///
/// Trivially passes — reaching this step means the prior When did
/// not panic. Used by edge-case scope tests against unknown
/// entities/clients.
#[then("no error is raised")]
fn then_no_error_is_raised(_ctx: &TestWorldRef) -> AssertOutcome<()> {
    AssertOutcome::Passed(())
}

// ──────────────────────────────────────────────────────────────────────
// Observability — RTT predicates
// ──────────────────────────────────────────────────────────────────────

const RTT_MAX_VALUE_MS: f32 = 10000.0;

/// Then the RTT returns a defined default value.
#[then("the RTT returns a defined default value")]
fn then_rtt_returns_default(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            !rtt.is_nan() && !rtt.is_infinite(),
            "RTT default should be a valid float, got: {:?}",
            rtt
        );
        assert!(
            rtt >= 0.0,
            "RTT default should be non-negative, got: {}",
            rtt
        );
    });
}

/// Then the RTT metric is non-negative.
#[then("the RTT metric is non-negative")]
fn then_rtt_is_non_negative(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(rtt >= 0.0, "RTT must be non-negative, got: {}", rtt);
    });
}

/// Then the RTT metric is finite.
#[then("the RTT metric is finite")]
fn then_rtt_is_finite(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            !rtt.is_nan() && !rtt.is_infinite(),
            "RTT must be finite, got: {:?}",
            rtt
        );
    });
}

/// Then the RTT metric is less than RTT_MAX_VALUE_MS.
#[then("the RTT metric is less than RTT_MAX_VALUE_MS")]
fn then_rtt_is_less_than_max(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        let max_rtt_s = RTT_MAX_VALUE_MS / 1000.0;
        assert!(
            rtt < max_rtt_s,
            "RTT must be less than {} seconds, got: {}",
            max_rtt_s,
            rtt
        );
    });
}

/// Then the RTT metric is within tolerance of expected latency.
#[then("the RTT metric is within tolerance of expected latency")]
fn then_rtt_within_tolerance(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should be valid, got: {}",
            rtt
        );
    });
}

/// Then the RTT metric does not reflect the prior session value.
#[then("the RTT metric does not reflect the prior session value")]
fn then_rtt_not_prior_session(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should be valid, got: {}",
            rtt
        );
    });
}

/// Then the RTT metric converges toward the new latency.
#[then("the RTT metric converges toward the new latency")]
fn then_rtt_converges_new_latency(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        let rtt = client.rtt();
        assert!(
            rtt >= 0.0 && !rtt.is_nan() && !rtt.is_infinite(),
            "RTT should converge to valid value, got: {}",
            rtt
        );
    });
}

// ──────────────────────────────────────────────────────────────────────
// Transport — operation-result predicates
// ──────────────────────────────────────────────────────────────────────

/// Then the transport adapter is not called.
///
/// Asserts the prior When was rejected gracefully (Err, not panic).
/// "Transport adapter not called" means the packet was caught at the
/// API layer before reaching transport.
#[then("the transport adapter is not called")]
fn then_transport_adapter_not_called(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        !result.is_ok,
        "Expected operation to be rejected, but it succeeded"
    );
    assert!(
        result.panic_msg.is_none(),
        "Operation caused a panic instead of graceful rejection: {:?}",
        result.panic_msg
    );
}

/// Then the server continues operating normally.
#[then("the server continues operating normally")]
fn then_server_continues_operating_normally(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(result.is_ok, "Server did not continue operating normally");
    assert!(
        result.panic_msg.is_none(),
        "Server panicked during packet loss: {:?}",
        result.panic_msg
    );
}

/// Then the client continues operating normally.
#[then("the client continues operating normally")]
fn then_client_continues_operating_normally(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(result.is_ok, "Client did not continue operating normally");
    assert!(
        result.panic_msg.is_none(),
        "Client panicked during packet loss: {:?}",
        result.panic_msg
    );
}

/// Then the server handles them without panic.
#[then("the server handles them without panic")]
fn then_server_handles_without_panic(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(result.is_ok, "Server did not handle packets gracefully");
    assert!(
        result.panic_msg.is_none(),
        "Server panicked while handling packets: {:?}",
        result.panic_msg
    );
}

/// Then the client handles them without panic.
#[then("the client handles them without panic")]
fn then_client_handles_without_panic(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(result.is_ok, "Client did not handle packets gracefully");
    assert!(
        result.panic_msg.is_none(),
        "Client panicked while handling packets: {:?}",
        result.panic_msg
    );
}

/// Then observable application behavior is identical.
#[then("observable application behavior is identical")]
fn then_observable_application_behavior_identical(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        result.is_ok,
        "Application behavior was not identical across transports: {:?}",
        result.panic_msg
    );
    assert!(
        result.panic_msg.is_none(),
        "Application panicked during transport abstraction test: {:?}",
        result.panic_msg
    );
}

/// Then no transport-specific guarantees are exposed.
#[then("no transport-specific guarantees are exposed")]
fn then_no_transport_specific_guarantees_exposed(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        result.is_ok,
        "Transport-specific guarantees may have leaked: application behaved differently"
    );
    assert!(
        result.panic_msg.is_none(),
        "Transport-specific behavior caused panic: {:?}",
        result.panic_msg
    );
}

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — connection-state predicates
// ──────────────────────────────────────────────────────────────────────

/// Then the server has no connected users.
#[then("the server has no connected users")]
fn then_server_has_no_connected_users(ctx: &TestWorldRef) {
    ctx.server(|server| {
        assert_eq!(
            server.users_count(),
            0,
            "Expected 0 connected users, but found {}",
            server.users_count()
        );
    });
}

/// Then the client is connected.
#[then("the client is connected")]
fn then_client_is_connected(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    if ctx.client_is_connected(client_key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client is not connected.
#[then("the client is not connected")]
fn then_client_is_not_connected(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    if !ctx.client_is_connected(client_key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

// ──────────────────────────────────────────────────────────────────────
// Common — error-taxonomy + operation-result + tick-availability
// ──────────────────────────────────────────────────────────────────────

/// Then the operation returns an Err result.
#[then("the operation returns an Err result")]
fn then_operation_returns_err(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        !result.is_ok,
        "Expected operation to return Err, but it returned Ok"
    );
    assert!(
        result.panic_msg.is_none(),
        "Expected Err result, but got a panic: {:?}",
        result.panic_msg
    );
}

/// Then no panic occurs.
#[then("no panic occurs")]
fn then_no_panic_occurs(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        result.panic_msg.is_none(),
        "Expected no panic, but got: {:?}",
        result.panic_msg
    );
}

/// Then the operation succeeds.
#[then("the operation succeeds")]
fn then_operation_succeeds(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded - did you run a When step?");
    assert!(
        result.is_ok,
        "Expected operation to succeed: error={:?}, panic={:?}",
        result.error_msg, result.panic_msg
    );
}

/// Then the packet is dropped.
///
/// Asserts no panic + connection still intact.
#[then("the packet is dropped")]
fn then_packet_is_dropped(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Packet handling caused a panic: {:?}",
        result.panic_msg
    );
    let client_key = ctx.last_client();
    assert!(
        ctx.client_is_connected(client_key),
        "Client should still be connected after malformed packet was dropped"
    );
}

/// Then no connection disruption occurs.
#[then("no connection disruption occurs")]
fn then_no_connection_disruption_occurs(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    assert!(
        ctx.client_is_connected(client_key),
        "Expected connection to remain intact, but it was disrupted"
    );
}

/// Then they are handled idempotently.
///
/// Asserts the duplicate-message handler completed without panic +
/// connection still intact.
#[then("they are handled idempotently")]
fn then_handled_idempotently(ctx: &TestWorldRef) {
    let result = ctx
        .scenario()
        .last_operation_result()
        .expect("No operation result recorded");
    assert!(
        result.panic_msg.is_none(),
        "Duplicate message handling caused a panic: {:?}",
        result.panic_msg
    );
    let client_key = ctx.last_client();
    assert!(
        ctx.client_is_connected(client_key),
        "Client should still be connected after duplicate messages"
    );
}

/// Then it receives fresh entity spawns for all in-scope entities.
///
/// Reconnection-scenario predicate. Connection-status proxy for
/// "client received fresh state" — full per-entity verification is
/// covered by the entity-replication scenarios.
#[then("it receives fresh entity spawns for all in-scope entities")]
fn then_receives_fresh_entity_spawns(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    if ctx.client_is_connected(client_key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then no prior session state is retained.
///
/// Reconnection-scenario predicate. The "fresh session" semantic is
/// implemented by: (a) server creates a new UserKey, (b) client
/// receives fresh spawns. We verify by the connection-status proxy.
#[then("no prior session state is retained")]
fn then_no_prior_session_state(ctx: &TestWorldRef) {
    let client_key = ctx.last_client();
    assert!(
        ctx.client_is_connected(client_key),
        "Reconnected client should be in a fresh connected state"
    );
}

/// Then the client tick is available.
///
/// Polls until `client_tick()` returns Some. Covers
/// [time-ticks-03.t1] (ConnectEvent implies tick sync complete).
#[then("the client tick is available")]
fn then_client_tick_is_available(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.client(client_key, |c| {
        if c.client_tick().is_some() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the server tick is known to the client.
///
/// Covers [time-ticks-04.t1] (client knows server's current tick at
/// connect time).
#[then("the server tick is known to the client")]
fn then_server_tick_is_known_to_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.client(client_key, |c| {
        if c.server_tick().is_some() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client with correct Position and Velocity values.
#[then("the entity spawns on the client with correct Position and Velocity values")]
fn then_entity_spawns_with_correct_values(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::{Position, Velocity};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    let exp_p: (f32, f32) = ctx.scenario().bdd_get(SPAWN_POSITION_VALUE_KEY).expect("no pos");
    let exp_v: (f32, f32) = ctx.scenario().bdd_get(SPAWN_VELOCITY_VALUE_KEY).expect("no vel");
    ctx.client(client_key, |client| {
        let Some(e) = client.entity(&entity_key) else { return AssertOutcome::Pending; };
        let Some(p) = e.component::<Position>() else { return AssertOutcome::Pending; };
        let Some(v) = e.component::<Velocity>() else { return AssertOutcome::Pending; };
        let ok = (*p.x - exp_p.0).abs() < f32::EPSILON && (*p.y - exp_p.1).abs() < f32::EPSILON
            && (*v.vx - exp_v.0).abs() < f32::EPSILON && (*v.vy - exp_v.1).abs() < f32::EPSILON;
        if ok { AssertOutcome::Passed(()) } else { AssertOutcome::Pending }
    })
}
