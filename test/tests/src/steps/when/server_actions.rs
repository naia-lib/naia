//! When-step bindings: server-initiated state changes.

use naia_test_harness::EntityKey;
use namako_engine::when;

use crate::steps::world_helpers::LAST_ENTITY_KEY;
use crate::TestWorldMut;

/// When the server adds the entity to the client's room.
///
/// Used by scope-propagation tests where the entity arrives in the
/// client's already-occupied room mid-scenario.
#[when("the server adds the entity to the client's room")]
fn when_server_adds_entity_to_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
        });
    });
}

/// When the server performs an idle tick.
///
/// Advances one server tick with no mutations. Used by update-
/// candidate-set tests to confirm idle ticks produce no dirty work.
#[when("the server performs an idle tick")]
fn when_server_performs_idle_tick(ctx: &mut TestWorldMut) {
    ctx.scenario_mut().mutate(|_| {});
}

/// When the server removes the entity from client A's scope.
///
/// Excludes the stored entity from client A's scope, triggering the
/// client to despawn it. Used by [server-events-09] tests.
#[when("the server removes the entity from client A's scope")]
fn when_server_removes_entity_from_client_a_scope(ctx: &mut TestWorldMut) {
    use naia_test_harness::ClientKey;
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.exclude(&entity_key);
            }
        });
    });
}

/// When the server removes the replicated component.
///
/// Removes Position from the stored entity. Used by world-integration
/// component-removal propagation tests.
#[when("the server removes the replicated component")]
fn when_server_removes_replicated_component(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.remove_component::<Position>();
            }
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Messaging — server message sends + responses
// ──────────────────────────────────────────────────────────────────────

/// When the server sends messages A B C on an ordered reliable channel.
#[when("the server sends messages A B C on an ordered reliable channel")]
fn when_server_sends_messages_abc_ordered(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_received_messages();
    scenario.clear_operation_result();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<OrderedChannel, _>(&client_key, &TestMessage::new(1));
            server.send_message::<OrderedChannel, _>(&client_key, &TestMessage::new(2));
            server.send_message::<OrderedChannel, _>(&client_key, &TestMessage::new(3));
        });
    });
    scenario.record_ok();
}

/// When the server sends message A on an ordered reliable channel.
#[when("the server sends message A on an ordered reliable channel")]
fn when_server_sends_message_a_ordered(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{OrderedChannel, TestMessage};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_received_messages();
    scenario.clear_operation_result();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<OrderedChannel, _>(&client_key, &TestMessage::new(1));
        });
    });
    scenario.record_ok();
}

/// When the server responds to the request.
///
/// Reads the pending RPC request and sends a `TestResponse`.
#[when("the server responds to the request")]
fn when_server_responds_to_request(ctx: &mut TestWorldMut) {
    use naia_shared::{GlobalResponseId, ResponseSendKey};
    use naia_test_harness::test_protocol::{RequestResponseChannel, TestRequest, TestResponse};
    let scenario = ctx.scenario_mut();
    let (response_id, _request): (GlobalResponseId, TestRequest) = scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((_client_key, response_id, request)) = server
                .read_request::<RequestResponseChannel, TestRequest>()
                .next()
            {
                return Some((response_id, request));
            }
            None
        })
    });
    let response_send_key: ResponseSendKey<TestResponse> = ResponseSendKey::new(response_id);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_response(&response_send_key, &TestResponse::new("test_result"));
        });
    });
    scenario.record_ok();
}

// ──────────────────────────────────────────────────────────────────────
// Entity replication — server inserts/updates components
// ──────────────────────────────────────────────────────────────────────

/// When the server inserts the replicated component.
///
/// Inserts a Position component on the stored entity. Covers
/// [client-events-06] (insert events fire when component added).
#[when("the server inserts the replicated component")]
fn when_server_inserts_replicated_component(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.insert_component(Position::new(42.0, 99.0));
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server updates the replicated component.
///
/// Updates Position to (100, 200) on the stored entity. Stores the
/// new value under `LAST_COMPONENT_VALUE_KEY`.
#[when("the server updates the replicated component")]
fn when_server_updates_replicated_component(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let new_value = (100.0_f32, 200.0_f32);
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = new_value.0;
                    *pos.y = new_value.1;
                }
            }
        });
    });
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, new_value);
    scenario.mutate(|_| {});
}

// ──────────────────────────────────────────────────────────────────────
// Priority accumulator — multi-entity actions
// ──────────────────────────────────────────────────────────────────────

/// When the server spawns N entities in one tick and scopes them for the client.
///
/// Used by AB-BDD-1 (spawn-burst drainage). Stores all entity keys
/// under `SPAWN_BURST_KEYS` for the matching Then assertion.
#[when("the server spawns {int} entities in one tick and scopes them for the client")]
fn when_server_spawns_n_entities_one_tick(ctx: &mut TestWorldMut, count: usize) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::SPAWN_BURST_KEYS;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();
    let keys: Vec<EntityKey> = scenario.mutate(|mctx| {
        let mut keys = Vec::with_capacity(count);
        mctx.server(|server| {
            for i in 0..count {
                let (ek, _) = server.spawn(|mut entity| {
                    entity.insert_component(Position::new(i as f32, i as f32));
                });
                keys.push(ek);
            }
            for ek in &keys {
                if let Some(mut e) = server.entity_mut(ek) {
                    e.enter_room(&room_key);
                }
            }
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                for ek in &keys {
                    scope.include(ek);
                }
            }
        });
        keys
    });
    scenario.bdd_store(SPAWN_BURST_KEYS, keys);
}

/// When the server sets the global priority gain on the last entity.
#[when("the server sets the global priority gain on the last entity to {float}")]
fn when_server_sets_global_gain_on_last_entity(ctx: &mut TestWorldMut, gain: f32) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.set_global_entity_gain(&entity_key, gain);
        });
    });
}

/// When the server mutates entity {label}'s component to x={int} y={int}.
///
/// `label` is "A" or "B"; resolves via [`entity_label_to_key_storage`].
/// Used by B-BDD-8 (per-entity convergence under cross-entity reorder).
#[when("the server mutates entity {word}'s component to x={int} y={int}")]
fn when_server_mutates_entity_component(
    ctx: &mut TestWorldMut,
    label: String,
    x: i32,
    y: i32,
) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::entity_label_to_key_storage;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(entity_label_to_key_storage(&label))
        .unwrap_or_else(|| panic!("entity '{}' not stored", label));
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = x as f32;
                    *pos.y = y as f32;
                }
            }
        });
    });
}
