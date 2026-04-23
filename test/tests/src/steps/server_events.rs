//! Step bindings for Server Events API contract (12_server_events_api.feature)
//!
//! These steps cover:
//!   - Server spawn event fires for in-scope user (server-events-07)
//!   - Server despawn event fires when entity leaves scope (server-events-09)
//!   - Server authority grant/reset events observable (server-events-XX)
//!   - Server publish event observable when client publishes entity (server-events-XX)

use naia_test_harness::{ClientKey, EntityKey, Position, ServerEntityAuthGrantEvent, ServerEntityAuthResetEvent, ServerPublishEntityEvent};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

fn client_key_storage(name: &str) -> String {
    format!("client_{}", name)
}

// ============================================================================
// Given Steps — Entity Scope Setup
// ============================================================================

/// Step: And a server-owned entity enters scope for client A
///
/// Spawns a server-owned entity, puts it in the room, and includes it in
/// client A's scope. Uses the standard room from `last_room()`.
/// This variant does NOT wait for replication — the next step asserts the event.
#[given("a server-owned entity enters scope for client A")]
fn given_server_owned_entity_enters_scope_for_client_a(ctx: &mut TestWorldMut) {
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

/// Step: And the server has observed a spawn event for client A (given phase)
///
/// Polls until the entity is in scope for client A on the server side.
/// `ServerSpawnEntityEvent` only fires for client-spawned entities; for
/// server-owned entities we use scope membership as the observable proxy.
/// Used as a precondition setup step.
#[given("the server has observed a spawn event for client A")]
fn given_server_has_observed_spawn_event_for_client_a(ctx: &mut TestWorldMut) {
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
                if in_scope { Some(()) } else { None }
            })
        },
    );

    scenario.allow_flexible_next();
}

// ============================================================================
// When Steps — Scope Changes
// ============================================================================

/// Step: When the server removes the entity from client A's scope
///
/// Excludes the last entity from client A's scope, triggering a despawn event.
#[when("the server removes the entity from client A's scope")]
fn when_server_removes_entity_from_client_a_scope(ctx: &mut TestWorldMut) {
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

// ============================================================================
// Then Steps — Server Event Assertions
// ============================================================================

/// Step: Then the server observes a spawn event for client A
///
/// Polls until the entity is in scope for client A.
/// `ServerSpawnEntityEvent` only fires for client-spawned entities; for
/// server-owned entities scope membership is the correct observable proxy.
/// Covers [server-events-07.t1]: entity is visible to client A on server side.
#[then("the server observes a spawn event for client A")]
fn then_server_observes_spawn_event_for_client_a(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.server(|s| {
        let in_scope = s
            .user_scope(&client_a)
            .map(|scope| scope.has(&entity_key))
            .unwrap_or(false);
        if in_scope {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server observes an authority grant event for client A
///
/// Polls until the server sees `ServerEntityAuthGrantEvent` for client A and
/// the last entity. Covers [server-events-XX.t1]: authority grant is observable
/// server-side when client is granted authority.
#[then("the server observes an authority grant event for client A")]
fn then_server_observes_authority_grant_event_for_client_a(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.server(|s| {
        let found = s
            .read_event::<ServerEntityAuthGrantEvent>()
            .map(|(ck, ek)| ck == client_a && ek == entity_key)
            .unwrap_or(false);
        if found {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server observes an authority reset event
///
/// Polls until the server sees `ServerEntityAuthResetEvent` for the last entity.
/// Covers [server-events-XX.t1]: authority reset is observable server-side when
/// a client releases authority.
#[then("the server observes an authority reset event")]
fn then_server_observes_authority_reset_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.server(|s| {
        let found = s
            .read_event::<ServerEntityAuthResetEvent>()
            .map(|ek| ek == entity_key)
            .unwrap_or(false);
        if found {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server observes a publish event for client A
///
/// Polls until the server sees `ServerPublishEntityEvent` for client A and the
/// last entity. Covers [server-events-XX.t1]: publish event is observable
/// server-side when a client makes its entity Public.
#[then("the server observes a publish event for client A")]
fn then_server_observes_publish_event_for_client_a(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.server(|s| {
        let found = s
            .read_event::<ServerPublishEntityEvent>()
            .map(|(ck, ek)| ck == client_a && ek == entity_key)
            .unwrap_or(false);
        if found {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the server observes a despawn event for client A
///
/// Polls until the entity is no longer in scope for client A.
/// `ServerDespawnEntityEvent` only fires for client-spawned entities; for
/// server-owned entities scope absence is the correct observable proxy.
/// Covers [server-events-09.t1]: entity leaves client A's visible set.
#[then("the server observes a despawn event for client A")]
fn then_server_observes_despawn_event_for_client_a(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.server(|s| {
        let in_scope = s
            .user_scope(&client_a)
            .map(|scope| scope.has(&entity_key))
            .unwrap_or(false);
        if !in_scope {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}
