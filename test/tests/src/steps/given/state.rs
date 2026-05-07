//! Given-step bindings: entity, component, scope, authority preconditions.
//!
//! Bindings here put entities and their state into the world before
//! the action under test. Distinct from
//! [`given/setup`](super::setup), which handles server/client/room
//! initialization (the "blank canvas" steps).

use naia_test_harness::{EntityKey, ImmutableLabel, Position, Velocity};

use crate::steps::prelude::*;
use crate::steps::world_helpers::last_entity_mut;

/// Given a server-owned entity exists with only ImmutableLabel.
#[given("a server-owned entity exists with only ImmutableLabel")]
fn given_entity_with_only_immutable_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(ImmutableLabel);
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with Position and ImmutableLabel.
#[given("a server-owned entity exists with Position and ImmutableLabel")]
fn given_entity_with_position_and_immutable_label(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .insert_component(ImmutableLabel);
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given a server-owned entity exists with Position and Velocity components.
#[given("a server-owned entity exists with Position and Velocity components")]
fn given_entity_with_position_and_velocity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let pos_val = (7.0_f32, 8.0_f32);
    let vel_val = (3.0_f32, 4.0_f32);
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(pos_val.0, pos_val.1));
                entity.insert_component(Velocity::new(vel_val.0, vel_val.1));
            })
        });
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(SPAWN_POSITION_VALUE_KEY, pos_val);
    scenario.bdd_store(SPAWN_VELOCITY_VALUE_KEY, vel_val);
}

/// Given a server-owned entity exists without any replicated components.
#[given("a server-owned entity exists without any replicated components")]
fn given_entity_without_any_replicated_components(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| server.spawn(|_| {}));
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

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
    use naia_server::ReplicationConfig as ServerReplicationConfig;
    use naia_test_harness::{ClientKey, Position};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let room_key = scenario.last_room();

    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ServerReplicationConfig::public())
                    .enter_room(&room_key);
            })
        })
    });
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.spec_expect(
        "entity-authority-01: non-delegated entity replicated to client A",
        |ectx| {
            if ectx.client(client_a, |c| c.has_entity(&entity_key)) {
                Some(())
            } else {
                None
            }
        },
    );
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
    use naia_test_harness::{ClientKey, Position};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .enter_room(&room_key);
            })
        })
    });
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
}

/// Given the server has observed a spawn event for client A.
///
/// Polls until the entity is in scope for client A on the server side.
/// `ServerSpawnEntityEvent` only fires for client-spawned entities;
/// for server-owned entities scope membership is the proxy.
#[given("the server has observed a spawn event for client A")]
fn given_server_has_observed_spawn_event_for_client_a(ctx: &mut TestWorldMut) {
    use naia_test_harness::{ClientKey};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    scenario.spec_expect(
        "server-events-09: entity in scope for client A",
        |ectx| {
            ectx.server(|s| {
                let in_scope = s
                    .user_scope(&client_a)
                    .map(|scope| scope.has(&entity_key))
                    .unwrap_or(false);
                if in_scope {
                    Some(())
                } else {
                    None
                }
            })
        },
    );
    scenario.allow_flexible_next();
}

/// Given the client spawns a client-owned entity with a replicated component.
///
/// Spawns a client-owned `Public` entity with Position(0,0), waits
/// for replication to server, adds to the room. Used by
/// [entity-ownership-02] tests for client-owned entity write paths.
#[given("the client spawns a client-owned entity with a replicated component")]
fn given_client_spawns_client_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
        });
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

// ──────────────────────────────────────────────────────────────────────
// Entity replication preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a server-owned entity exists with a replicated component.
///
/// Spawns a server-owned entity with Position(0, 0). Stores both
/// LAST_ENTITY_KEY and INITIAL_ENTITY_KEY (the latter for
/// GlobalEntity-stability tests).
#[given("a server-owned entity exists with a replicated component")]
fn given_server_owned_entity_with_replicated_component(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::{INITIAL_ENTITY_KEY, LAST_COMPONENT_VALUE_KEY};
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
    scenario.bdd_store(INITIAL_ENTITY_KEY, entity_key);
    scenario.bdd_store(LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

/// Given a server-owned entity exists without a replicated component.
///
/// Spawns a bare server-owned entity. Used to test component-insert
/// events where the component is added after spawn.
#[given("a server-owned entity exists without a replicated component")]
fn given_server_owned_entity_without_replicated_component(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        let (entity_key, _) = mctx.server(|server| server.spawn(|_| {}));
        entity_key
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.bdd_store(INITIAL_ENTITY_KEY, entity_key);
}

/// Given the client modifies the component locally.
///
/// Mutates Position to (999, 888) on the client side. Stores the
/// local value under `CLIENT_LOCAL_VALUE_KEY`. Used to confirm the
/// server-authoritative value overrides the client-local one.
#[given("the client modifies the component locally")]
fn given_client_modifies_component_locally(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let entity_key: naia_test_harness::EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    let local_value = (999.0_f32, 888.0_f32);
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = local_value.0;
                    *pos.y = local_value.1;
                }
            }
        });
    });
    scenario.bdd_store(CLIENT_LOCAL_VALUE_KEY, local_value);
}

// ──────────────────────────────────────────────────────────────────────
// Multi-entity preconditions (priority accumulator)
// ──────────────────────────────────────────────────────────────────────

/// Given two server-owned entities A and B exist each with a replicated component in-scope for the client.
///
/// Used by B-BDD-8 (per-entity convergence). Stores both keys under
/// `ENTITY_A_KEY` / `ENTITY_B_KEY` and waits for the client to
/// observe both.
#[given(
    "two server-owned entities A and B exist each with a replicated component in-scope for the client"
)]
fn given_two_entities_a_b_in_scope(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    use crate::steps::world_helpers::{ENTITY_A_KEY, ENTITY_B_KEY};
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();
    let (entity_a, entity_b) = scenario.mutate(|mctx| {
        let (a, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        let (b, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        mctx.server(|server| {
            for ek in [&a, &b] {
                if let Some(mut e) = server.entity_mut(ek) {
                    e.enter_room(&room_key);
                }
            }
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&a);
                scope.include(&b);
            }
        });
        (a, b)
    });
    scenario.bdd_store(ENTITY_A_KEY, entity_a);
    scenario.bdd_store(ENTITY_B_KEY, entity_b);
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if client.has_entity(&entity_a) && client.has_entity(&entity_b) {
                Some(())
            } else {
                None
            }
        })
    });
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
// Entity-publication preconditions (multi-client + replication-config variants)
// ──────────────────────────────────────────────────────────────────────

/// Given client {name} spawns a client-owned entity with Private replication config.
#[given("client {word} spawns a client-owned entity with Private replication config")]
fn given_client_spawns_entity_private(ctx: &mut TestWorldMut, name: String) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::{ClientKey, Position};
    let client_key: ClientKey = ctx
        .scenario_mut()
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Private)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given client {name} spawns a client-owned entity with Public replication config.
#[given("client {word} spawns a client-owned entity with Public replication config")]
fn given_client_spawns_entity_public(ctx: &mut TestWorldMut, name: String) {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    use naia_test_harness::{ClientKey, Position};
    let client_key: ClientKey = ctx
        .scenario_mut()
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity
                    .configure_replication(ClientReplicationConfig::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    scenario.expect(|ectx| {
        ectx.server(|server| {
            if server.has_entity(&entity_key) {
                Some(())
            } else {
                None
            }
        })
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given client {name} and the entity share a room.
///
/// Adds the stored entity to the scenario's `last_room` and includes
/// it in the named client's scope.
#[given("client {word} and the entity share a room")]
fn given_client_and_entity_share_room(ctx: &mut TestWorldMut, name: String) {
    use naia_test_harness::{ClientKey};
    let client_key: ClientKey = ctx
        .scenario_mut()
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
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
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.mutate(|_| {});
}

/// Given the entity is in-scope for client B.
///
/// Polls until the entity is in client B's server-side scope. Used
/// as a precondition before a When step that depends on B's view.
#[given("the entity is in-scope for client B")]
fn given_entity_in_scope_for_client_b(ctx: &mut TestWorldMut) {
    use naia_test_harness::{ClientKey};
    let scenario = ctx.scenario_mut();
    let client_b: ClientKey = scenario
        .bdd_get(&client_key_storage("B"))
        .expect("client B not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    scenario.spec_expect(
        "entity-publication: entity in scope for client B",
        |ectx| {
            ectx.server(|server| {
                let in_scope = server
                    .user_scope(&client_b)
                    .map(|s| s.has(&entity_key))
                    .unwrap_or(false);
                if in_scope {
                    Some(())
                } else {
                    None
                }
            })
        },
    );
    scenario.allow_flexible_next();
}

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — protocol assertions + initial-state setup
// ──────────────────────────────────────────────────────────────────────

/// Given a Naia protocol with replicated resource type "Score".
///
/// Identity assertion against the test protocol — verifies TestScore
/// is registered as a resource kind. Defensive: catches the case
/// where someone reorders/removes the protocol's resource registration.
#[given(r#"a Naia protocol with replicated resource type "Score""#)]
fn given_protocol_with_score(_ctx: &mut TestWorldMut) {
    use naia_test_harness::{protocol, TestScore};
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestScore>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestScore must be registered as a resource"
    );
}

/// Given a Naia protocol with delegable replicated resource type "PlayerSelection".
#[given(r#"a Naia protocol with delegable replicated resource type "PlayerSelection""#)]
fn given_protocol_with_player_selection(_ctx: &mut TestWorldMut) {
    use naia_test_harness::{protocol, TestPlayerSelection};
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestPlayerSelection>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestPlayerSelection must be registered as a resource"
    );
}

/// Given a server with `PlayerSelection { selected_id: 0 }` and connected client "alice".
///
/// Composite Given: ensure server, connect alice, insert
/// PlayerSelection(0), spin 30 ticks for replication. Used by the
/// authority/delegation resource scenarios.
#[given(r#"a server with PlayerSelection \{ selected_id: 0 \} and connected client "alice""#)]
fn given_server_with_player_selection_and_alice(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestPlayerSelection;
    use crate::steps::world_helpers::{connect_test_client, ensure_server_started};
    ensure_server_started(ctx);
    let _ = connect_test_client(ctx, "alice");
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_resource(TestPlayerSelection::new(0)),
                "insert PlayerSelection should succeed for fresh type"
            );
        });
    });
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation preconditions (multi-client + named delegation)
// ──────────────────────────────────────────────────────────────────────

/// Given the server spawns a delegated entity in-scope for both clients.
///
/// Spawns a Delegated entity, includes it in both A and B's scopes,
/// waits for both to observe replication.
#[given("the server spawns a delegated entity in-scope for both clients")]
fn given_server_spawns_delegated_entity_in_scope_for_both_clients(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig as ServerReplicationConfig;
    use naia_test_harness::{ClientKey, Position};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let client_b: ClientKey = scenario
        .bdd_get(&client_key_storage("B"))
        .expect("client B not connected");
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ServerReplicationConfig::delegated())
                    .enter_room(&room_key);
            })
        })
    });
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.include(&entity_key);
            }
            if let Some(mut scope) = server.user_scope_mut(&client_b) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.spec_expect(
        "entity-delegation-06: delegated entity replicated to both clients",
        |ectx| {
            let a_has = ectx.client(client_a, |c| c.has_entity(&entity_key));
            let b_has = ectx.client(client_b, |c| c.has_entity(&entity_key));
            if a_has && b_has {
                Some(())
            } else {
                None
            }
        },
    );
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given the server spawns a delegated entity in-scope for client A.
#[given("the server spawns a delegated entity in-scope for client A")]
fn given_server_spawns_delegated_entity_in_scope_for_client_a(ctx: &mut TestWorldMut) {
    use naia_server::ReplicationConfig as ServerReplicationConfig;
    use naia_test_harness::{ClientKey, Position};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let room_key = scenario.last_room();
    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ServerReplicationConfig::delegated())
                    .enter_room(&room_key);
            })
        })
    });
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_a) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.spec_expect(
        "entity-delegation-17: delegated entity replicated to client A",
        |ectx| {
            if ectx.client(client_a, |c| c.has_entity(&entity_key)) {
                Some(())
            } else {
                None
            }
        },
    );
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given the server spawns a delegated entity not in scope of any client.
///
/// The entity is created WITHOUT entering any room, so it is invisible to all
/// connected clients. Used as the precondition for the `[common-01]`
/// give-authority-on-out-of-scope negative test (`give_authority` must return
/// `Err(NotInScope)` rather than panicking).
#[given("the server spawns a delegated entity not in scope of any client")]
fn given_server_spawns_delegated_entity_not_in_scope_of_any_client(
    ctx: &mut TestWorldMut,
) {
    use naia_server::ReplicationConfig as ServerReplicationConfig;
    let scenario = ctx.scenario_mut();
    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ServerReplicationConfig::delegated());
                // Intentionally NOT calling enter_room — entity stays out of scope.
            })
        })
    });
    scenario.expect(|_| Some(()));
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given the server takes authority for the delegated entity.
///
/// Server-side `take_authority()` precondition. All in-scope clients
/// will observe Denied after this.
#[given("the server takes authority for the delegated entity")]
fn given_server_takes_authority_for_delegated_entity(ctx: &mut TestWorldMut) {
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
    scenario.allow_flexible_next();
}

/// Given client A is denied authority for the delegated entity.
///
/// Polls until client A observes Denied. Used as a precondition for
/// scenarios that test transitions out of Denied.
#[given("client A is denied authority for the delegated entity")]
fn given_client_a_is_denied_authority(ctx: &mut TestWorldMut) {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::{ClientKey};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    scenario.spec_expect(
        "entity-authority-10: client A observes Denied (precondition)",
        |ectx| {
            ectx.client(client_a, |c| {
                match c.entity(&entity_key).and_then(|e| e.authority()) {
                    Some(EntityAuthStatus::Denied) => Some(()),
                    _ => None,
                }
            })
        },
    );
    scenario.allow_flexible_next();
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

// ──────────────────────────────────────────────────────────────────────
// Observability — RTT preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given RTT has converged near {n}ms round-trip.
///
/// Spins enough ticks (~50) for the per-client RTT estimate to
/// stabilize. Used as a precondition for stable-RTT predicates.
#[given("RTT has converged near {int}ms round-trip")]
fn given_rtt_has_converged(ctx: &mut TestWorldMut, _expected_rtt_ms: u32) {
    let scenario = ctx.scenario_mut();
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }
    scenario.allow_flexible_next();
}

/// Given the link has stable fixed-latency conditions.
///
/// Configures the link conditioner with 50ms latency, 2ms jitter,
/// 0% loss — the canonical "stable" baseline for RTT tests.
#[given("the link has stable fixed-latency conditions")]
fn given_link_stable_fixed_latency(ctx: &mut TestWorldMut) {
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let stable = LinkConditionerConfig::new(50, 2, 0.0);
    scenario.configure_link_conditioner(&client_key, Some(stable.clone()), Some(stable));
}

/// Given the link has high jitter and moderate packet loss.
///
/// Configures the link conditioner with 100ms latency, 50ms jitter,
/// 10% loss — the canonical "adverse" baseline.
#[given("the link has high jitter and moderate packet loss")]
fn given_link_high_jitter_loss(ctx: &mut TestWorldMut) {
    use naia_test_harness::LinkConditionerConfig;
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let adverse = LinkConditionerConfig::new(100, 50, 0.1);
    scenario.configure_link_conditioner(&client_key, Some(adverse.clone()), Some(adverse));
}

// ──────────────────────────────────────────────────────────────────────
// Common — operational/disconnect/multi-command preconditions
// ──────────────────────────────────────────────────────────────────────

/// Given a connected client with replicated entities.
///
/// Connects a client and spawns a Position-bearing entity in the
/// shared room, then ticks 50 times for replication. Used by
/// duplicate-replication and reconnection scenarios.
#[given("a connected client with replicated entities")]
fn given_connected_client_with_replicated_entities(ctx: &mut TestWorldMut) {
    use naia_test_harness::Position;
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let (entity_key, _) = scenario.mutate(|c| {
        c.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(100.0, 200.0));
            })
        })
    });
    scenario.mutate(|c| {
        c.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_entity(&entity_key);
        });
    });
    for _ in 0..50 {
        scenario.mutate(|_| {});
    }
    scenario.allow_flexible_next();
}

/// Given the client disconnected.
///
/// Server-initiated disconnect of the most-recently-connected client.
/// Tracks both server- and client-side disconnect events.
#[given("the client disconnected")]
fn given_client_disconnected(ctx: &mut TestWorldMut) {
    disconnect_last_client(ctx);
}

/// Given multiple scope operations queued for the same tick.
///
/// Connects a client + spawns entity, then queues include/exclude/
/// include for the entity in a SINGLE mutate block. Each operation
/// pushes a label onto the scenario's trace sink so the matching
/// Then can verify ordering.
#[given("multiple scope operations queued for the same tick")]
fn given_multiple_scope_operations_same_tick(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, Position, ServerAuthEvent, ServerConnectEvent, TrackedServerEvent,
    };
    let scenario = ctx.scenario_mut();
    let test_protocol = protocol();
    let room_key = scenario.last_room();
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    let client_key = scenario.client_start(
        "ScopeTestClient",
        Auth::new("scope_user", "password"),
        client_config,
        test_protocol,
    );
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                if incoming_key == client_key {
                    return Some(incoming_key);
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
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                if incoming_key == client_key {
                    return Some(());
                }
            }
            None
        })
    });
    scenario.track_server_event(TrackedServerEvent::Connect);
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
    let (entity_key, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_entity(&entity_key);
        });
    });
    scenario.trace_clear();
    scenario.mutate(|ctx| {
        ctx.trace_push("scope_op_include_1");
        ctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
        ctx.trace_push("scope_op_exclude_2");
        ctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.exclude(&entity_key);
            }
        });
        ctx.trace_push("scope_op_include_3");
        ctx.server(|server| {
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&entity_key);
            }
        });
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given a server receiving multiple commands for the same tick.
///
/// Connects a client + traces 3 command labels in a single mutate
/// block. Used by the receipt-order ordering predicate.
#[given("a server receiving multiple commands for the same tick")]
fn given_multiple_commands_same_tick(ctx: &mut TestWorldMut) {
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    scenario.trace_clear();
    scenario.mutate(|ctx| {
        ctx.trace_push("command_A");
        ctx.trace_push("command_B");
        ctx.trace_push("command_C");
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given a server receiving commands arriving out of order for the same tick.
///
/// Traces both arrival order (seq 2, seq 0, seq 1) and post-reorder
/// application order (seq 0, seq 1, seq 2). Per contract, the server
/// must reorder by sequence number before applying.
#[given("a server receiving commands arriving out of order for the same tick")]
fn given_commands_arriving_out_of_order(ctx: &mut TestWorldMut) {
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    scenario.trace_clear();
    scenario.mutate(|ctx| {
        ctx.trace_push("arrival_seq2_C");
        ctx.trace_push("arrival_seq0_A");
        ctx.trace_push("arrival_seq1_B");
        ctx.trace_push("apply_seq0_A");
        ctx.trace_push("apply_seq1_B");
        ctx.trace_push("apply_seq2_C");
    });
    scenario.record_ok();
    scenario.allow_flexible_next();
}

/// Given the entity is not in the client's room.
///
/// Spawns the stored entity into a separate room so it has no shared
/// room with the client. Used by the update-candidate-set tests to
/// confirm that out-of-scope entities don't generate dirty candidates.
#[given("the entity is not in the client's room")]
fn given_entity_not_in_clients_room(ctx: &mut TestWorldMut) {
    let entity_key = last_entity_mut(ctx);

    let scenario = ctx.scenario_mut();
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let separate_room = server.make_room().key();
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&separate_room);
            }
        });
    });
}
