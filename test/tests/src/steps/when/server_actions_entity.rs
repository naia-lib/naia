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
    let client_a = named_client_mut(ctx, "A");
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    let scenario = ctx.scenario_mut();
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

/// When the server inserts a replicated component on the stored entity.
///
/// Uses the harness `insert_component_on` API to add Position(1,1) to an
/// already-registered entity.  Covers Gap-B (dynamic insert after spawn).
#[when("the server inserts a replicated component on the entity")]
fn when_server_inserts_component_on_entity(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.insert_component_on::<Position>(&entity_key, Position::new(1.0, 1.0));
        });
    });
}

/// When the server removes a replicated component from the stored entity.
///
/// Uses the harness `remove_component_from` API. Covers Gap-B (dynamic
/// remove after spawn).
#[when("the server removes a replicated component from the entity")]
fn when_server_removes_component_from_entity(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.remove_component_from::<Position>(&entity_key);
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

/// When the server sends messages S1 S2 S3 on a sequenced channel.
///
/// Sends values 1, 2, 3 on the `SequencedChannel` (SequencedReliable).
/// Used to verify "latest wins" semantics (messaging-07, messaging-10).
#[when("the server sends messages S1 S2 S3 on a sequenced channel")]
fn when_server_sends_messages_s1_s2_s3_sequenced(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{SequencedChannel, TestMessage};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<SequencedChannel, _>(&client_key, &TestMessage::new(1));
            server.send_message::<SequencedChannel, _>(&client_key, &TestMessage::new(2));
            server.send_message::<SequencedChannel, _>(&client_key, &TestMessage::new(3));
        });
    });
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
            assert!(server.insert_resource(TestScore::new(0, 0), false), "insert Score should succeed");
        });
    });
}

/// When the server inserts `MatchState { phase: N }` as static.
#[when(r#"the server inserts MatchState \{ phase: {int} \} as static"#)]
fn when_server_inserts_matchstate_as_static(ctx: &mut TestWorldMut, phase: u8) {
    use naia_test_harness::TestMatchState;
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(server.insert_static_resource(TestMatchState::new(phase)), "insert MatchState should succeed");
        });
    });
}

/// When the server removes MatchState.
#[when("the server removes MatchState")]
fn when_server_removes_matchstate(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestMatchState;
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(server.remove_resource::<TestMatchState>(), "remove MatchState should succeed");
        });
    });
}

/// When the server attempts to re-insert Score with new values.
///
/// Asserts the insert returns false (resource already exists) so the
/// Then steps can verify the original value is unchanged. Covers the
/// idempotency / re-insert-rejection contract.
#[when(r#"the server attempts to re-insert Score \{ home: {int}, away: {int} \}"#)]
fn when_server_attempts_reinsert_score(ctx: &mut TestWorldMut, home: u32, away: u32) {
    use naia_test_harness::TestScore;
    let scenario = ctx.scenario_mut();
    let accepted = scenario.mutate(|c| {
        c.server(|server| server.insert_resource(TestScore::new(home, away), false))
    });
    assert!(!accepted, "re-insert of an existing Score must return false (ResourceAlreadyExists)");
}

// ──────────────────────────────────────────────────────────────────────
// Tick-buffered messaging — server-side read and inject (messaging-13/14)
// ──────────────────────────────────────────────────────────────────────

/// When the server reads tick-buffered messages for that tick.
///
/// Waits (via spec_expect) for the server to advance past the tick stored
/// under `TICK_BUFFER_TICK_KEY`, then reads the tick buffer and stores
/// the message count under `TICK_BUFFER_COUNT_KEY`. Used by messaging-13.
#[when("the server reads tick-buffered messages for that tick")]
fn when_server_reads_tick_buffered_messages(ctx: &mut TestWorldMut) {
    use naia_shared::sequence_greater_than;
    use naia_test_harness::test_protocol::{TickBufferedChannel, TestMessage};
    let tick: naia_shared::Tick = ctx
        .scenario_mut()
        .bdd_get(TICK_BUFFER_TICK_KEY)
        .expect("no tick stored — did the client send tick-buffered messages?");
    let scenario = ctx.scenario_mut();
    scenario.spec_expect("tick-buffer: wait for server to advance past target tick", |ectx| {
        let now = ectx.server(|s| s.current_tick());
        sequence_greater_than(now, tick).then_some(())
    });
    let count = scenario.mutate(|mctx| {
        mctx.server(|s| {
            let mut tb = s.receive_tick_buffer_messages(&tick);
            tb.read::<TickBufferedChannel, TestMessage>().len()
        })
    });
    scenario.bdd_store(TICK_BUFFER_COUNT_KEY, count);
}

/// When a tick-buffered message is injected for an expired tick.
///
/// Advances 10 ticks, then uses inject_tick_buffer_message with
/// message_tick = host_tick - 100 (past the buffer window). Stores
/// the rejection outcome (true if rejected) under
/// `TICK_BUFFER_REJECTED_KEY`. Used by messaging-14.
#[when("a tick-buffered message is injected for an expired tick")]
fn when_tick_buffered_message_injected_for_expired_tick(ctx: &mut TestWorldMut) {
    use naia_test_harness::test_protocol::{TickBufferedChannel, TestMessage};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    for _ in 0..10 {
        scenario.mutate(|_| {});
    }
    let accepted = scenario.mutate(|mctx| {
        mctx.server(|s| {
            let host_tick = s.current_tick();
            let msg_tick = host_tick.wrapping_sub(100);
            s.inject_tick_buffer_message::<TickBufferedChannel, TestMessage>(
                &client_key,
                &host_tick,
                &msg_tick,
                &TestMessage::new(99),
            )
        })
    });
    scenario.bdd_store(TICK_BUFFER_REJECTED_KEY, !accepted);
}

