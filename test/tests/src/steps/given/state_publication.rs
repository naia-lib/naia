//! Given-step bindings: publication (Public/Private replication) preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::last_entity_mut;

use naia_test_harness::EntityKey;

// ──────────────────────────────────────────────────────────────────────
// Entity-publication preconditions (multi-client + replication-config variants)
// ──────────────────────────────────────────────────────────────────────

fn spawn_client_entity_with_config(ctx: &mut TestWorldMut, name: &str, config: naia_client::ReplicationConfig) {
    use naia_test_harness::Position;
    let client_key = named_client_mut(ctx, name);
    let scenario = ctx.scenario_mut();
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn(|mut entity| {
                entity.configure_replication(config).insert_component(Position::new(0.0, 0.0));
            })
        })
    });
    scenario.expect(|ectx| {
        ectx.server(|server| server.has_entity(&entity_key).then_some(()))
    });
    scenario.bdd_store(LAST_ENTITY_KEY, entity_key);
    scenario.allow_flexible_next();
}

/// Given client {client} spawns a client-owned entity with Private replication config.
#[given("client {client} spawns a client-owned entity with Private replication config")]
fn given_client_spawns_entity_private(ctx: &mut TestWorldMut, name: ClientName) {
    spawn_client_entity_with_config(ctx, name.as_ref(), naia_client::ReplicationConfig::Private);
}

/// Given client {client} spawns a client-owned entity with Public replication config.
#[given("client {client} spawns a client-owned entity with Public replication config")]
fn given_client_spawns_entity_public(ctx: &mut TestWorldMut, name: ClientName) {
    spawn_client_entity_with_config(ctx, name.as_ref(), naia_client::ReplicationConfig::Public);
}

/// Given client {client} and the entity share a room.
///
/// Adds the stored entity to the scenario's `last_room` and includes
/// it in the named client's scope.
#[given("client {client} and the entity share a room")]
fn given_client_and_entity_share_room(ctx: &mut TestWorldMut, name: ClientName) {
    let client_key = named_client_mut(ctx, name.as_ref());
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
    let client_b = named_client_mut(ctx, "B");
    let entity_key = last_entity_mut(ctx);
    let scenario = ctx.scenario_mut();
    scenario.spec_expect("entity-publication: entity in scope for client B", |ectx|
        ectx.server(|s| s.user_scope(&client_b)
            .map(|sc| sc.has(&entity_key))
            .unwrap_or(false)
            .then_some(())));
    scenario.allow_flexible_next();
}
