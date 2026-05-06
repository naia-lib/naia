//! Given-step bindings: entity, component, scope, authority preconditions.
//!
//! Bindings here put entities and their state into the world before
//! the action under test. Distinct from
//! [`given/setup`](super::setup), which handles server/client/room
//! initialization (the "blank canvas" steps).

use naia_test_harness::{ImmutableLabel, Position, Velocity};
use namako_engine::given;

use crate::steps::world_helpers::{
    LAST_ENTITY_KEY, SPAWN_POSITION_VALUE_KEY, SPAWN_VELOCITY_VALUE_KEY,
};
use crate::TestWorldMut;

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
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
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
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
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
    use naia_test_harness::{ClientKey, EntityKey};
    let scenario = ctx.scenario_mut();
    let client_a: ClientKey = scenario
        .bdd_get(&crate::steps::world_helpers::client_key_storage("A"))
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
    scenario.bdd_store(crate::steps::world_helpers::LAST_COMPONENT_VALUE_KEY, (0.0_f32, 0.0_f32));
}

/// Given the entity is not in the client's room.
///
/// Spawns the stored entity into a separate room so it has no shared
/// room with the client. Used by the update-candidate-set tests to
/// confirm that out-of-scope entities don't generate dirty candidates.
#[given("the entity is not in the client's room")]
fn given_entity_not_in_clients_room(ctx: &mut TestWorldMut) {
    use naia_test_harness::EntityKey;
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            let separate_room = server.make_room().key();
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&separate_room);
            }
        });
    });
}
