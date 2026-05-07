//! Given-step bindings: scope / room / authority preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;

use naia_test_harness::EntityKey;
use crate::steps::world_helpers::last_entity_mut;

// ──────────────────────────────────────────────────────────────────────
// Authority/scope/ownership preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given the server spawns a non-delegated entity in-scope for client A.
///
/// Spawns a server-owned entity with `ReplicationConfig::Public` (NOT
/// Delegated), enters it in the room, includes it in client A's scope,
/// waits for replication. Used by [entity-authority-01] tests where
/// authority is undefined for non-delegated entities.
#[given("the server spawns a non-delegated entity in-scope for client A")]
fn given_server_spawns_non_delegated_entity_in_scope_for_client_a(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig as SRC;
    use naia_test_harness::{ClientKey, Position};
    use crate::steps::world_helpers::named_client_mut;
    let client_a: ClientKey = named_client_mut(ctx, "A");
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|c| c.server(|s|
        s.spawn(|mut e| { e.insert_component(Position::new(0.0, 0.0))
            .configure_replication(SRC::public()).enter_room(&room_key); })));
    scenario.mutate(|c| c.server(|s| {
        if let Some(mut scope) = s.user_scope_mut(&client_a) { scope.include(&entity_key); }
    }));
    scenario.spec_expect("entity-authority-01: non-delegated entity replicated", |ectx|
        ectx.client(client_a, |c| c.has_entity(&entity_key)).then_some(()));
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given a server-owned entity enters scope for client A.
///
/// Spawns a server-owned entity, puts it in the room, includes it in
/// client A's scope. Does NOT wait for replication — used as
/// precondition for tests asserting the spawn-event timing.
#[given("a server-owned entity enters scope for client A")]
fn given_server_owned_entity_enters_scope_for_client_a(ctx: &mut TestWorldMut) {
    use crate::steps::world_helpers::named_client_mut;
    use crate::steps::world_helpers_connect::spawn_position_entity_in_scope;
    let client_a = named_client_mut(ctx, "A");
    spawn_position_entity_in_scope(ctx, client_a);
}

/// Given the server has observed a spawn event for client A.
///
/// Polls until the entity is in scope for client A on the server side.
/// `ServerSpawnEntityEvent` only fires for client-spawned entities;
/// for server-owned entities scope membership is the proxy.
#[given("the server has observed a spawn event for client A")]
fn given_server_has_observed_spawn_event_for_client_a(ctx: &mut TestWorldMut) {
    use crate::steps::world_helpers::named_client_mut;
    let client_a = named_client_mut(ctx, "A");
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.spec_expect("server-events-09: entity in scope for client A", |ectx|
        ectx.server(|s| s.user_scope(&client_a)
            .map(|scope| scope.has(&entity_key))
            .unwrap_or(false)
            .then_some(())));
    scenario.allow_flexible_next();
}

/// Given the client spawns a client-owned entity with a replicated component.
///
/// Spawns a client-owned `Public` entity with Position(0,0), waits
/// for replication to server, adds to the room. Used by
/// [entity-ownership-02] tests for client-owned entity write paths.
#[given("the client spawns a client-owned entity with a replicated component")]
fn given_client_spawns_client_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    use naia_client::ReplicationConfig as CRC;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();
    let entity_key = scenario.mutate(|c| c.client(client_key, |cl|
        cl.spawn(|mut e| { e.configure_replication(CRC::Public).insert_component(Position::new(0.0, 0.0)); })));
    scenario.expect(|ectx| ectx.server(|s| s.has_entity(&entity_key).then_some(())));
    scenario.mutate(|c| c.server(|s| {
        if let Some(mut e) = s.entity_mut(&entity_key) { e.enter_room(&room_key); }
    }));
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}


// ──────────────────────────────────────────────────────────────────────
// Scope-exit (Persist) entity preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a server-owned entity exists with ScopeExit::Persist configured.
///
/// Spawns a Public entity with Position(0,0) and `persist_on_scope_exit()`.
/// Used as the baseline for ScopeExit::Persist tests.
#[given("a server-owned entity exists with ScopeExit::Persist configured")]
fn given_persist_entity(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with ScopeExit::Persist configured without ImmutableLabel.
#[given("a server-owned entity exists with ScopeExit::Persist configured without ImmutableLabel")]
fn given_persist_entity_without_label(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with ScopeExit::Persist configured with ImmutableLabel.
#[given("a server-owned entity exists with ScopeExit::Persist configured with ImmutableLabel")]
fn given_persist_entity_with_label(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig;
    use naia_test_harness::{ImmutableLabel, Position};
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .insert_component(ImmutableLabel)
                    .configure_replication(ReplicationConfig::public().persist_on_scope_exit());
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}


// ──────────────────────────────────────────────────────────────────────
// Entity-scope preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a server-owned entity exists.
///
/// Bare server-owned entity with Position(0, 0). Stored under
/// `LAST_ENTITY_KEY`. Distinct from
/// `a server-owned entity exists with a replicated component` —
/// this one's `Position` IS the replicated component, but the
/// scenario phrasing emphasizes existence rather than the component.
#[given("a server-owned entity exists")]
fn given_server_owned_entity_exists(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given the client owns an entity.
///
/// Client spawns a Public entity with Position(0, 0). Spins 50
/// ticks for replication to land on the server before storing.
#[given("the client owns an entity")]
fn given_client_owns_entity(ctx: &mut TestWorldMut) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given the client and entity share a room.
#[given("the client and entity share a room")]
fn given_client_and_entity_share_room_singleton(ctx: &mut TestWorldMut) {
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

/// Given the client and entity do not share a room.
#[given("the client and entity do not share a room")]
fn given_client_and_entity_do_not_share_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut room) = server.room_mut(&room_key) {
                if room.has_entity(&entity_key) {
                    room.remove_entity(&entity_key);
                }
            }
        });
    });
}

/// Given the entity is in-scope for the client.
///
/// Includes the entity in the client's scope and waits until the
/// client observes the entity locally.
#[given("the entity is in-scope for the client")]
fn given_entity_in_scope_for_client(ctx: &mut TestWorldMut) {
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
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if client.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
}

/// Given the entity is out-of-scope for the client.
#[given("the entity is out-of-scope for the client")]
fn given_entity_out_of_scope_for_client(ctx: &mut TestWorldMut) {
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

/// Given the server excludes the entity for the client (precondition).
///
/// Distinct from the When variant — this one is a precondition step
/// after which other Givens may run.
#[given("the server excludes the entity for the client")]
fn given_server_excludes_entity_for_client(ctx: &mut TestWorldMut) {
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

/// Given the entity is not in any room.
#[given("the entity is not in any room")]
fn given_entity_not_in_any_room(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let room_keys = server.room_keys();
            for room_key in room_keys {
                if let Some(mut room) = server.room_mut(&room_key) {
                    if room.has_entity(&entity_key) {
                        room.remove_entity(&entity_key);
                    }
                }
            }
        });
    });
    scenario.mutate(|_| {});
}

