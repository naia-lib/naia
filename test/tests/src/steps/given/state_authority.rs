//! Given-step bindings: authority delegation (multi-client) preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;

use naia_test_harness::{EntityKey, Position};
use crate::steps::world_helpers::last_entity_mut;

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation preconditions (multi-client + named delegation)
// ──────────────────────────────────────────────────────────────────────

/// Given the server spawns a delegated entity in-scope for both clients.
///
/// Spawns a Delegated entity, includes it in both A and B's scopes,
/// waits for both to observe replication.
#[given("the server spawns a delegated entity in-scope for both clients")]
fn given_server_spawns_delegated_entity_in_scope_for_both_clients(ctx: &mut TestWorldMut) {
    use crate::steps::world_helpers::named_client_mut;
    use crate::steps::world_helpers_connect::spawn_delegated_entity_in_scope;
    let client_a = named_client_mut(ctx, "A");
    let client_b = named_client_mut(ctx, "B");
    let entity_key = spawn_delegated_entity_in_scope(ctx, &[client_a, client_b]);
    let scenario = ctx.scenario_mut();
    scenario.spec_expect("entity-delegation-06: replicated to both clients", |ectx|
        (ectx.client(client_a, |c| c.has_entity(&entity_key))
            && ectx.client(client_b, |c| c.has_entity(&entity_key))).then_some(()));
    scenario.allow_flexible_next();
}

/// Given the server spawns a delegated entity in-scope for client A.
#[given("the server spawns a delegated entity in-scope for client A")]
fn given_server_spawns_delegated_entity_in_scope_for_client_a(ctx: &mut TestWorldMut) {
    use crate::steps::world_helpers::named_client_mut;
    use crate::steps::world_helpers_connect::spawn_delegated_entity_in_scope;
    let client_a = named_client_mut(ctx, "A");
    let entity_key = spawn_delegated_entity_in_scope(ctx, &[client_a]);
    let scenario = ctx.scenario_mut();
    scenario.spec_expect("entity-delegation-17: replicated to client A", |ectx|
        ectx.client(client_a, |c| c.has_entity(&entity_key)).then_some(()));
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
    let client_a = named_client_mut(ctx, "A");
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");
    let scenario = ctx.scenario_mut();
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

