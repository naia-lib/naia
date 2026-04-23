//! Step bindings for Client Events API contract (13_client_events_api.feature)
//!
//! These steps cover:
//!   - Client receives spawn event when entity enters scope (client-events-04)
//!   - Client receives despawn/spawn events on scope leave/re-enter (client-events-09)
//!   - Client receives component update event via Events API (client-events-07)
//!   - Client receives component remove event via Events API (client-events-08)
//!   - Client receives component insert event via Events API (client-events-06)

use naia_test_harness::{ClientKey, ClientSpawnEntityEvent, EntityKey};
use namako_engine::codegen::AssertOutcome;
use namako_engine::then;

use crate::TestWorldRef;

const LAST_ENTITY_KEY: &str = "last_entity";

// ============================================================================
// Then Steps — Client Event Assertions
// ============================================================================

/// Step: Then the client receives a spawn event for the entity
///
/// Polls until the last client receives a ClientSpawnEntityEvent for the
/// last entity. Covers [client-events-04.t1]: spawn is the first event for
/// an entity lifetime; [client-events-09.t1]: scope re-enter emits Spawn.
#[then("the client receives a spawn event for the entity")]
fn then_client_receives_spawn_event_for_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        let found = c
            .read_event::<ClientSpawnEntityEvent>()
            .map(|ek| ek == entity_key)
            .unwrap_or(false);
        if found {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client receives a despawn event for the entity
///
/// Polls until the last client no longer has the entity in its world.
/// `ClientDespawnEntityEvent` is consumed by process_despawn_events() before
/// step closures run, so entity absence is the correct observable proxy.
/// Covers [client-events-09.t1]: scope leave removes entity from client world.
#[then("the client receives a despawn event for the entity")]
fn then_client_receives_despawn_event_for_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if !c.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client receives a component update event for the entity
///
/// Polls until the client's Events API surfaces a component update event for
/// the last entity. Covers [client-events-07.t1]: component update events are
/// one-shot per applied change and observable via the Events API.
#[then("the client receives a component update event for the entity")]
fn then_client_receives_component_update_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if c.has_update_event_for_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client receives a component insert event for the entity
///
/// Polls until the client's Events API surfaces a component insert event for
/// the last entity. Covers [client-events-06.t1]: component insert events fire
/// when a replicated component is added to an in-scope entity.
#[then("the client receives a component insert event for the entity")]
fn then_client_receives_component_insert_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if c.has_insert_event_for_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the client receives a component remove event for the entity
///
/// Polls until the client's Events API surfaces a component remove event for
/// the last entity. Covers [client-events-08.t1]: component remove events are
/// one-shot per applied removal and observable via the Events API.
#[then("the client receives a component remove event for the entity")]
fn then_client_receives_component_remove_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_key, |c| {
        if c.has_remove_event_for_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}
