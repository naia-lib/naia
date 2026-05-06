//! Then-step bindings: observable state predicates.
//!
//! State assertions check the system's *current* observable state —
//! number of connected clients, which entities a client sees, what
//! authority status a client holds, etc. Distinct from
//! [`event_assertions`](super::event_assertions) which assert on
//! the *history* of emitted events.

use namako_engine::then;

use crate::TestWorldRef;

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
) -> namako_engine::codegen::AssertOutcome<()> {
    let depth = ctx.scenario().scope_change_queue_len();
    if depth == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
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
) -> namako_engine::codegen::AssertOutcome<()> {
    let count = ctx.scenario().total_dirty_update_count();
    if count == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
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
) -> namako_engine::codegen::AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 0 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Expected 0 global diff-handler receivers, got {}",
            snapshot.global_receivers
        ))
    }
}

/// Then the global diff handler has 1 receiver.
#[then("the global diff handler has 1 receiver")]
fn then_global_diff_handler_has_one_receiver(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    let snapshot = ctx.scenario().diff_handler_snapshot();
    if snapshot.global_receivers == 1 {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position, Velocity};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() && entity.has_component::<Velocity>() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.has_component::<Position>() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            if entity.component::<Position>().is_none() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{ClientKey, EntityKey};
    let scenario = ctx.scenario();
    let second_client: ClientKey = scenario
        .bdd_get(crate::steps::world_helpers::SECOND_CLIENT_KEY)
        .expect("second client not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(second_client, |c| {
        if c.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{ClientKey, EntityKey};
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                None => namako_engine::codegen::AssertOutcome::Passed(()),
                Some(status) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "expected None authority for non-delegated entity, got {:?}",
                    status
                )),
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    match ctx
        .scenario()
        .bdd_get::<bool>(crate::steps::world_helpers::LAST_REQUEST_ERROR_KEY)
    {
        Some(true) => namako_engine::codegen::AssertOutcome::Passed(()),
        Some(false) => namako_engine::codegen::AssertOutcome::Failed(
            "expected request_authority to return Err for non-delegated entity, got Ok".to_string(),
        ),
        None => namako_engine::codegen::AssertOutcome::Failed("no request result stored".to_string()),
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity ownership
// ──────────────────────────────────────────────────────────────────────

/// Then the entity owner is the client.
#[then("the entity owner is the client")]
fn then_entity_owner_is_client(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, EntityOwner};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.owner() {
                EntityOwner::Client(_) => namako_engine::codegen::AssertOutcome::Passed(()),
                other => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "Expected EntityOwner::Client for owned entity, got {:?}",
                    other
                )),
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the entity owner is the server.
#[then("the entity owner is the server")]
fn then_entity_owner_is_server(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, EntityOwner};
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if entity.owner() == EntityOwner::Server {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Failed(format!(
                    "Expected entity owner to be Server, but was {:?}",
                    entity.owner()
                ))
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the server no longer has the entity.
///
/// Covers [entity-ownership-08.t1] (owner disconnect despawns).
#[then("the server no longer has the entity")]
fn then_server_no_longer_has_entity(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.server(|server| {
        if server.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Pending
        } else {
            namako_engine::codegen::AssertOutcome::Passed(())
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
) -> namako_engine::codegen::AssertOutcome<()> {
    let rejected: bool = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::WRITE_REJECTED_KEY)
        .unwrap_or(false);
    if rejected {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(
            "Expected write to be rejected, but server state was modified".to_string(),
        )
    }
}

/// Then the server observes the component update.
///
/// Polls until server-side Position equals the value stored under
/// `LAST_COMPONENT_VALUE_KEY` by the matching When binding.
#[then("the server observes the component update")]
fn then_server_observes_component_update(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected: (f32, f32) = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    ctx.server(|server| {
        if let Some(entity) = server.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                if (*pos.x - expected.0).abs() < f32::EPSILON
                    && (*pos.y - expected.1).abs() < f32::EPSILON
                {
                    namako_engine::codegen::AssertOutcome::Passed(())
                } else {
                    namako_engine::codegen::AssertOutcome::Pending
                }
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let client_key = ctx.last_client();
    let mut received: Vec<u32> = Vec::new();
    ctx.client(client_key, |client| {
        for msg in client.read_message::<OrderedChannel, TestMessage>() {
            received.push(msg.value);
        }
    });
    if received.len() < 3 {
        return namako_engine::codegen::AssertOutcome::Pending;
    }
    if received == [1, 2, 3] {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Messages received out of order. Expected [1, 2, 3] (A, B, C), got {:?}",
            received
        ))
    }
}

/// Then the client receives message A exactly once.
#[then("the client receives message A exactly once")]
fn then_client_receives_message_a_exactly_once(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let client_key = ctx.last_client();
    let mut received: Vec<u32> = Vec::new();
    ctx.client(client_key, |client| {
        for msg in client.read_message::<OrderedChannel, TestMessage>() {
            received.push(msg.value);
        }
    });
    if received.is_empty() {
        return namako_engine::codegen::AssertOutcome::Pending;
    }
    if received == [1] {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else if received.len() > 1 {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Message A received multiple times. Expected [1], got {:?}",
            received
        ))
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
            "Wrong message received. Expected [1] (A), got {:?}",
            received
        ))
    }
}

/// Then the client receives the response for that request.
#[then("the client receives the response for that request")]
fn then_client_receives_response(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::ResponseReceiveKey;
    use naia_test_harness::test_protocol::TestResponse;
    use crate::steps::world_helpers::RESPONSE_RECEIVE_KEY;
    let client_key = ctx.last_client();
    let scenario = ctx.scenario();
    let response_key: Option<ResponseReceiveKey<TestResponse>> = scenario.bdd_get(RESPONSE_RECEIVE_KEY);
    let Some(response_key) = response_key else {
        return namako_engine::codegen::AssertOutcome::Failed(
            "No response receive key was stored - did the client send a request?".to_string(),
        );
    };
    ctx.client(client_key, |client| {
        if client.has_response(&response_key) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() {
                namako_engine::codegen::AssertOutcome::Passed(())
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the client observes the component update.
#[then("the client observes the component update")]
fn then_client_observes_component_update(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected: (f32, f32) = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                if (*pos.x - expected.0).abs() < f32::EPSILON
                    && (*pos.y - expected.1).abs() < f32::EPSILON
                {
                    namako_engine::codegen::AssertOutcome::Passed(())
                } else {
                    namako_engine::codegen::AssertOutcome::Pending
                }
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the client observes the server value.
///
/// Used after `Given the client modifies the component locally` —
/// asserts that the server-authoritative value overrides the
/// client-local modification.
#[then("the client observes the server value")]
fn then_client_observes_server_value(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let server_value: (f32, f32) = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY)
        .expect("No server component value stored");
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if let Some(pos) = entity.component::<Position>() {
                if (*pos.x - server_value.0).abs() < f32::EPSILON
                    && (*pos.y - server_value.1).abs() < f32::EPSILON
                {
                    namako_engine::codegen::AssertOutcome::Passed(())
                } else {
                    namako_engine::codegen::AssertOutcome::Pending
                }
            } else {
                namako_engine::codegen::AssertOutcome::Pending
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the entity GlobalEntity remains unchanged.
///
/// EntityKey is the harness abstraction over Naia's GlobalEntity.
/// Stable identity throughout an entity's lifetime is the contract.
#[then("the entity GlobalEntity remains unchanged")]
fn then_entity_global_entity_remains_unchanged(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let initial: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::INITIAL_ENTITY_KEY)
        .expect("No initial entity key stored");
    let current: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No current entity key stored");
    if initial == current {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Failed(format!(
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    use crate::steps::world_helpers::SPAWN_BURST_KEYS;
    let client_key = ctx.last_client();
    let keys: Vec<EntityKey> = ctx
        .scenario()
        .bdd_get(SPAWN_BURST_KEYS)
        .expect("spawn-burst keys missing");
    if keys.len() != expected {
        return namako_engine::codegen::AssertOutcome::Failed(format!(
            "stored {} burst keys but scenario expected {}",
            keys.len(),
            expected
        ));
    }
    ctx.client(client_key, |client| {
        if keys.iter().all(|k| client.has_entity(k)) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the global priority gain on the last entity is {float}.
#[then("the global priority gain on the last entity is {float}")]
fn then_global_gain_on_last_entity_is(
    ctx: &TestWorldRef,
    expected: f32,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.server(|server| match server.global_entity_gain(&entity_key) {
        Some(g) if (g - expected).abs() < f32::EPSILON => {
            namako_engine::codegen::AssertOutcome::Passed(())
        }
        Some(g) => namako_engine::codegen::AssertOutcome::Failed(format!(
            "global gain is {} but expected {}",
            g, expected
        )),
        None => namako_engine::codegen::AssertOutcome::Failed(format!(
            "no gain override is set (expected {})",
            expected
        )),
    })
}

/// Then the client eventually sees the last entity.
#[then("the client eventually sees the last entity")]
fn then_client_eventually_sees_last_entity(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    use crate::steps::world_helpers::entity_label_to_key_storage;
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(entity_label_to_key_storage(&label))
        .unwrap_or_else(|| panic!("entity '{}' not stored", label));
    let (ex, ey) = (x as f32, y as f32);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        if (*pos.x - ex).abs() < f32::EPSILON && (*pos.y - ey).abs() < f32::EPSILON {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::EntityKey;
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Failed(
                "Entity was despawned on client despite ScopeExit::Persist".into(),
            )
        }
    })
}

/// Then the client entity position is still 0.0.
///
/// Confirms no update leaked through while the entity was Paused.
#[then("the client entity position is still 0.0")]
fn then_client_entity_position_still_zero(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Failed(
                "Entity absent on client despite ScopeExit::Persist".into(),
            );
        };
        let Some(pos) = entity.component::<Position>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        if (*pos.x - 0.0).abs() < f32::EPSILON {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Failed(format!(
                "Position updated while entity was out-of-scope: expected x=0, got x={}",
                *pos.x,
            ))
        }
    })
}

/// Then the client entity position becomes 100.0.
///
/// Polling — confirms accumulated updates from the Paused period
/// arrive after re-entry.
#[then("the client entity position becomes 100.0")]
fn then_client_entity_position_becomes_hundred(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        if (*pos.x - 100.0).abs() < f32::EPSILON {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the client entity has ImmutableLabel.
#[then("the client entity has ImmutableLabel")]
fn then_client_entity_has_label(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        if entity.has_component::<ImmutableLabel>() {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the client entity does not have ImmutableLabel.
#[then("the client entity does not have ImmutableLabel")]
fn then_client_entity_no_label(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        if !entity.has_component::<ImmutableLabel>() {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
    use naia_test_harness::{ClientKey, EntityKey};
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage(label))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", label));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
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
) -> namako_engine::codegen::AssertOutcome<()> {
    if !check_entity_in_scope(ctx, "B") {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage(&label))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", label));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected = match config_name.as_str() {
        "Public" => ClientReplicationConfig::Public,
        "Private" => ClientReplicationConfig::Private,
        "Delegated" => ClientReplicationConfig::Delegated,
        other => {
            return namako_engine::codegen::AssertOutcome::Failed(format!(
                "Unknown replication config: '{}'",
                other
            ))
        }
    };
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(config) if config == expected => namako_engine::codegen::AssertOutcome::Passed(()),
                Some(other) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "Expected replication_config {:?}, got {:?}",
                    expected, other
                )),
                None => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — client-side observability
// ──────────────────────────────────────────────────────────────────────

/// Then the client's Score is present.
#[then("the client's Score is present")]
fn then_client_has_score(ctx: &TestWorldRef) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    if ctx.client(key, |c| c.has_resource::<TestScore>()) {
        namako_engine::codegen::AssertOutcome::Passed(())
    } else {
        namako_engine::codegen::AssertOutcome::Pending
    }
}

/// Then the client's Score.home equals 0.
#[then("the client's Score.home equals 0")]
fn then_client_score_home_0(ctx: &TestWorldRef) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.home)) {
        Some(0) => namako_engine::codegen::AssertOutcome::Passed(()),
        _ => namako_engine::codegen::AssertOutcome::Pending,
    }
}

/// Then the client's Score.away equals 0.
#[then("the client's Score.away equals 0")]
fn then_client_score_away_0(ctx: &TestWorldRef) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.away)) {
        Some(0) => namako_engine::codegen::AssertOutcome::Passed(()),
        _ => namako_engine::codegen::AssertOutcome::Pending,
    }
}

/// Then alice's authority status for PlayerSelection is "Granted".
#[then(r#"alice's authority status for PlayerSelection is "Granted""#)]
fn then_alice_auth_granted(ctx: &TestWorldRef) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::TestPlayerSelection;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource_authority_status::<TestPlayerSelection>()) {
        Some(EntityAuthStatus::Granted) => namako_engine::codegen::AssertOutcome::Passed(()),
        _ => namako_engine::codegen::AssertOutcome::Pending,
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Granted) => namako_engine::codegen::AssertOutcome::Passed(()),
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    namako_engine::codegen::AssertOutcome::Pending
                }
                Some(other) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "client {}: expected Granted, got {:?}",
                    name, other
                )),
                None => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Denied) => namako_engine::codegen::AssertOutcome::Passed(()),
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    namako_engine::codegen::AssertOutcome::Pending
                }
                Some(other) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "client {}: expected Denied, got {:?}",
                    name, other
                )),
                None => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
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
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    namako_engine::codegen::AssertOutcome::Passed(())
                }
                _ => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the delegated entity is no longer in client A's world.
///
/// Covers [entity-delegation-13.t1] (entity leaves scope on exclude).
#[then("the delegated entity is no longer in client A's world")]
fn then_delegated_entity_is_no_longer_in_client_a_world(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{ClientKey, EntityKey};
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_a, |c| {
        if c.has_entity(&entity_key) {
            namako_engine::codegen::AssertOutcome::Pending
        } else {
            namako_engine::codegen::AssertOutcome::Passed(())
        }
    })
}

/// Then client A observes Delegated replication config for the entity.
///
/// Covers [entity-delegation-17.t1] (delegation observable from client).
#[then("client A observes Delegated replication config for the entity")]
fn then_client_a_observes_delegated_replication_config(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(ClientReplicationConfig::Delegated) => {
                    namako_engine::codegen::AssertOutcome::Passed(())
                }
                Some(other) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "expected Delegated replication config, got {:?}",
                    other
                )),
                None => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then client A observes Available authority status for the entity.
#[then("client A observes Available authority status for the entity")]
fn then_client_a_observes_available_authority_status(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::{ClientKey, EntityKey};
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    namako_engine::codegen::AssertOutcome::Passed(())
                }
                Some(other) => namako_engine::codegen::AssertOutcome::Failed(format!(
                    "expected Available authority status, got {:?}",
                    other
                )),
                None => namako_engine::codegen::AssertOutcome::Pending,
            }
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client with correct Position and Velocity values.
#[then("the entity spawns on the client with correct Position and Velocity values")]
fn then_entity_spawns_with_correct_values(
    ctx: &TestWorldRef,
) -> namako_engine::codegen::AssertOutcome<()> {
    use naia_test_harness::{EntityKey, Position, Velocity};
    let client_key = ctx.last_client();
    let scenario = ctx.scenario();
    let entity_key: EntityKey = scenario
        .bdd_get(crate::steps::world_helpers::LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let expected_pos: (f32, f32) = scenario
        .bdd_get(crate::steps::world_helpers::SPAWN_POSITION_VALUE_KEY)
        .expect("No position value stored");
    let expected_vel: (f32, f32) = scenario
        .bdd_get(crate::steps::world_helpers::SPAWN_VELOCITY_VALUE_KEY)
        .expect("No velocity value stored");
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let Some(vel) = entity.component::<Velocity>() else {
            return namako_engine::codegen::AssertOutcome::Pending;
        };
        let pos_ok = (*pos.x - expected_pos.0).abs() < f32::EPSILON
            && (*pos.y - expected_pos.1).abs() < f32::EPSILON;
        let vel_ok = (*vel.vx - expected_vel.0).abs() < f32::EPSILON
            && (*vel.vy - expected_vel.1).abs() < f32::EPSILON;
        if pos_ok && vel_ok {
            namako_engine::codegen::AssertOutcome::Passed(())
        } else {
            namako_engine::codegen::AssertOutcome::Pending
        }
    })
}
