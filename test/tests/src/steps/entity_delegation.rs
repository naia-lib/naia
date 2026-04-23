//! Step bindings for Entity Delegation contract (10_entity_delegation.feature)
//!
//! These steps cover:
//!   - Server-spawned delegated entity setup (in-scope for named clients)
//!   - Authority request from named clients
//!   - Granted / Denied authority assertions
//!   - [entity-delegation-06]: first-request-wins arbitration template
//!
//! Named client connection ("client {word} connects") is provided by
//! `entity_publication` steps, which use the same `client_{name}` BDD storage key.

use naia_server::ReplicationConfig;
use naia_shared::EntityAuthStatus;
use naia_test_harness::{ClientKey, EntityKey, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

fn client_key_storage(name: &str) -> String {
    format!("client_{}", name)
}

// ============================================================================
// Given Steps — Delegated Entity Setup
// ============================================================================

/// Step: And the server spawns a delegated entity in-scope for both clients
///
/// Spawns a server-owned entity with `ReplicationConfig::Delegated`, enters it
/// in the room, includes it in both clients A and B scopes, and waits until
/// both clients have the entity replicated locally.
#[namako_engine::given("the server spawns a delegated entity in-scope for both clients")]
fn given_server_spawns_delegated_entity_in_scope_for_both_clients(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let client_b: ClientKey = scenario
        .bdd_get(&client_key_storage("B"))
        .expect("client B not connected");
    let room_key = scenario.last_room();

    // Spawn delegated entity and add it to the room
    let (entity_key, ()) = scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.spawn(|mut entity| {
                entity
                    .insert_component(Position::new(0.0, 0.0))
                    .configure_replication(ReplicationConfig::Delegated)
                    .enter_room(&room_key);
            })
        })
    });

    // Include the entity in both user scopes (separate mutate to avoid double-borrow)
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

    // Wait until both clients have the entity replicated
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

// ============================================================================
// When Steps — Authority Requests
// ============================================================================

/// Step: When client {word} requests authority for the delegated entity
///
/// The named client calls `request_authority()` on the entity. This immediately
/// transitions the client's local status to Requested (optimistic pending).
#[when("client {word} requests authority for the delegated entity")]
fn when_client_requests_authority(ctx: &mut TestWorldMut, name: String) {
    let scenario = ctx.scenario_mut();
    let client_key: ClientKey = scenario
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                entity
                    .request_authority()
                    .expect("request_authority should not error for in-scope delegated entity");
            } else {
                panic!(
                    "client {} cannot see delegated entity — not in scope",
                    name
                );
            }
        });
    });
}

// ============================================================================
// Then Steps — Authority Status Assertions
// ============================================================================

/// Assert that the named client has authority status Granted for the last entity.
fn assert_authority_status(
    ctx: &TestWorldRef,
    name: &str,
    expected: EntityAuthStatus,
) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage(name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(status) if status == expected => AssertOutcome::Passed(()),
                // Optimistically Requested or still Available — wait for server round-trip
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Pending
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected {:?}, got {:?}",
                    name, expected, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then client {word} is granted authority for the delegated entity
///
/// Polls until the named client observes EntityAuthStatus::Granted.
/// This covers [entity-delegation-06.t1]: first in-scope request wins.
#[then("client {word} is granted authority for the delegated entity")]
fn then_client_is_granted_authority(ctx: &TestWorldRef, name: String) -> AssertOutcome<()> {
    assert_authority_status(ctx, &name, EntityAuthStatus::Granted)
}

/// Step: Then client {word} is denied authority for the delegated entity
///
/// Polls until the named client observes EntityAuthStatus::Denied.
/// This covers [entity-delegation-07.t1]: denied while another holds authority.
#[then("client {word} is denied authority for the delegated entity")]
fn then_client_is_denied_authority(ctx: &TestWorldRef, name: String) -> AssertOutcome<()> {
    assert_authority_status(ctx, &name, EntityAuthStatus::Denied)
}
