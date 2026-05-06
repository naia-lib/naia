//! When-step bindings: server-initiated state changes.

use naia_test_harness::EntityKey;

use crate::steps::prelude::*;
use crate::steps::world_helpers::last_entity_mut;

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
        .bdd_get(&client_key_storage("A"))
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
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
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
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
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
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
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
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.set_global_entity_gain(&entity_key, gain);
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Scope-exit (Persist) — server actions
// ──────────────────────────────────────────────────────────────────────

/// When the server updates the entity position to 100 100.
///
/// Used by ScopeExit::Persist tests to verify that updates issued
/// while the client is excluded ARE delivered on re-entry.
#[when("the server updates the entity position to 100 100")]
fn when_server_updates_entity_position(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = 100.0;
                    *pos.y = 100.0;
                }
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server globally despawns the entity.
///
/// Despawns the entity from the server world entirely. Used by the
/// ScopeExit::Persist test that asserts global despawn propagates
/// even while the entity was Paused on the client.
#[when("the server globally despawns the entity")]
fn when_server_globally_despawns_entity(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.despawn();
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server inserts ImmutableLabel on the entity.
#[when("the server inserts ImmutableLabel on the entity")]
fn when_server_inserts_label(ctx: &mut TestWorldMut) {
    use naia_test_harness::ImmutableLabel;
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.insert_component(ImmutableLabel);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server removes ImmutableLabel from the entity.
#[when("the server removes ImmutableLabel from the entity")]
fn when_server_removes_label(ctx: &mut TestWorldMut) {
    use naia_test_harness::ImmutableLabel;
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.remove_component::<ImmutableLabel>();
            }
        });
    });
    scenario.mutate(|_| {});
}

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — server-side resource ops
// ──────────────────────────────────────────────────────────────────────

/// When the server inserts `Score { home: 0, away: 0 }` as a dynamic resource.
#[when(r#"the server inserts Score \{ home: 0, away: 0 \} as a dynamic resource"#)]
fn when_server_inserts_score_dynamic(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestScore;
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_resource(TestScore::new(0, 0)),
                "insert Score should succeed"
            );
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation — server-side authority + scope ops
// ──────────────────────────────────────────────────────────────────────

/// When the server removes the delegated entity from client A's scope.
///
/// Excludes the entity from A's scope, triggering authority release
/// per [entity-delegation-13].
#[when("the server removes the delegated entity from client A's scope")]
fn when_server_removes_delegated_entity_from_client_a_scope(ctx: &mut TestWorldMut) {
    use naia_test_harness::ClientKey;
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.exclude(&entity_key);
            }
        });
    });
}

/// When the server takes authority for the delegated entity.
#[when("the server takes authority for the delegated entity")]
fn when_server_takes_authority(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity
                    .take_authority()
                    .expect("take_authority should succeed for server");
            }
        });
    });
}

/// When the server releases authority for the delegated entity.
#[when("the server releases authority for the delegated entity")]
fn when_server_releases_authority(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity
                    .release_authority()
                    .expect("release_authority should succeed for server");
            }
        });
    });
}

// ──────────────────────────────────────────────────────────────────────
// Entity-scope — server scope-mut operations
// ──────────────────────────────────────────────────────────────────────

/// When the server includes the entity for the client.
#[when("the server includes the entity for the client")]
fn when_server_includes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server excludes the entity for the client.
#[when("the server excludes the entity for the client")]
fn when_server_excludes_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server includes an unknown entity for the client.
///
/// Edge-case test — invalid EntityKey should be a no-op.
#[when("the server includes an unknown entity for the client")]
fn when_server_includes_unknown_entity_for_client(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let unknown_entity_key = naia_test_harness::EntityKey::invalid();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&unknown_entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// When the server includes the entity for an unknown client.
///
/// Edge-case test — invalid ClientKey should be a no-op
/// (`user_scope_mut` returns None for unknown clients).
#[when("the server includes the entity for an unknown client")]
fn when_server_includes_entity_for_unknown_client(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    let unknown_client_key = naia_test_harness::ClientKey::invalid();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&unknown_client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

// ──────────────────────────────────────────────────────────────────────
// Transport — server outbound packet sends
// ──────────────────────────────────────────────────────────────────────

/// When the server sends a packet within the MTU limit.
#[when("the server sends a packet within the MTU limit")]
fn when_server_sends_packet_within_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{TestMessage, UnreliableChannel};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<UnreliableChannel, _>(&client_key, &TestMessage::new(42));
        });
    });
    scenario.record_ok();
}

/// When the server attempts to send a packet exceeding MTU.
///
/// Catches any panic and records the outcome — the contract is that
/// oversized packets are rejected gracefully, not by panicking.
#[when("the server attempts to send a packet exceeding MTU")]
fn when_server_attempts_send_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{LargeTestMessage, UnreliableChannel};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.clear_operation_result();
    let result = catch_unwind(AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<UnreliableChannel, _>(
                    &client_key,
                    &LargeTestMessage::new(1000),
                );
            });
        });
    }));
    match result {
        Ok(()) => scenario.record_err("Oversized packet rejected"),
        Err(p) => scenario.record_panic(panic_payload_to_string(p)),
    }
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
