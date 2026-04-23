//! Step bindings for Entity Delegation contract (10_entity_delegation.feature)
//!
//! These steps cover:
//!   - Server-spawned delegated entity setup (in-scope for named clients)
//!   - Authority request from named clients
//!   - Granted / Denied / Available authority assertions
//!   - [entity-delegation-06]: first-request-wins arbitration
//!   - [entity-delegation-11]: release returns Denied clients to Available
//!   - [entity-delegation-13]: losing scope releases authority
//!   - [entity-delegation-14]: disconnect releases authority
//!   - [entity-delegation-17]: Delegated config observable from client

use naia_client::ReplicationConfig as ClientReplicationConfig;
use naia_server::ReplicationConfig as ServerReplicationConfig;
use naia_shared::EntityAuthStatus;
use naia_test_harness::{ClientKey, EntityKey, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

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
#[given("the server spawns a delegated entity in-scope for both clients")]
fn given_server_spawns_delegated_entity_in_scope_for_both_clients(ctx: &mut TestWorldMut) {
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
                    .configure_replication(ServerReplicationConfig::Delegated)
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

/// Step: And the server spawns a delegated entity in-scope for client A
///
/// Single-client variant: spawns a delegated entity in A's room and scope,
/// waits until client A has the entity replicated locally.
#[given("the server spawns a delegated entity in-scope for client A")]
fn given_server_spawns_delegated_entity_in_scope_for_client_a(ctx: &mut TestWorldMut) {
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
                    .configure_replication(ServerReplicationConfig::Delegated)
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

/// Step: And the server takes authority for the delegated entity (given phase)
///
/// Server calls take_authority() and waits for all clients in the scenario to
/// observe Denied. Used in Contract 11 Scenario 03 setup.
#[given("the server takes authority for the delegated entity")]
fn given_server_takes_authority_for_delegated_entity(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

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

/// Step: And client A is denied authority for the delegated entity (given phase)
///
/// Polls until client A observes Denied — used as a given precondition.
#[given("client A is denied authority for the delegated entity")]
fn given_client_a_is_denied_authority(ctx: &mut TestWorldMut) {
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

// ============================================================================
// When Steps — Authority Requests / Release / Disconnect
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

/// Step: When client {word} releases authority for the delegated entity
///
/// The named client calls `release_authority()` on the entity. Transitions
/// from Granted → Releasing → Available.
#[when("client {word} releases authority for the delegated entity")]
fn when_client_releases_authority(ctx: &mut TestWorldMut, name: String) {
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
                    .release_authority()
                    .expect("release_authority should not error when Granted");
            } else {
                panic!(
                    "client {} cannot see delegated entity — not in scope",
                    name
                );
            }
        });
    });
}

/// Step: When the server removes the delegated entity from client A's scope
///
/// Server calls `user_scope_mut.exclude()` for client A, causing the entity
/// to leave A's scope. This triggers authority release per [entity-delegation-13].
#[when("the server removes the delegated entity from client A's scope")]
fn when_server_removes_delegated_entity_from_client_a_scope(ctx: &mut TestWorldMut) {
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

/// Step: When client A disconnects from the server
///
/// Server-initiated disconnect for client A, triggering authority release
/// per [entity-delegation-14].
#[when("client A disconnects from the server")]
fn when_client_a_disconnects_from_server(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.disconnect_user(&client_a);
        });
    });
}

/// Step: When the server takes authority for the delegated entity
///
/// Server calls `take_authority()` on the entity, becoming the authority holder.
/// All in-scope clients will see Denied.
#[when("the server takes authority for the delegated entity")]
fn when_server_takes_authority(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

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

/// Step: When the server releases authority for the delegated entity
///
/// Server calls `release_authority()` on the entity, resetting authority to
/// Available for all clients. Covers [entity-authority-10] server override/reset.
#[when("the server releases authority for the delegated entity")]
fn when_server_releases_authority(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

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

// ============================================================================
// Then Steps — Authority Status Assertions
// ============================================================================

/// Helper: assert a named client observes the expected EntityAuthStatus.
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
/// Covers [entity-delegation-06.t1]: first in-scope request wins.
#[then("client {word} is granted authority for the delegated entity")]
fn then_client_is_granted_authority(ctx: &TestWorldRef, name: String) -> AssertOutcome<()> {
    assert_authority_status(ctx, &name, EntityAuthStatus::Granted)
}

/// Step: Then client {word} is denied authority for the delegated entity
///
/// Polls until the named client observes EntityAuthStatus::Denied.
/// Covers [entity-delegation-07.t1]: denied while another holds authority.
#[then("client {word} is denied authority for the delegated entity")]
fn then_client_is_denied_authority(ctx: &TestWorldRef, name: String) -> AssertOutcome<()> {
    // Allow Requested as a transient state while waiting to be Denied
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Denied) => AssertOutcome::Passed(()),
                // Requested and Available are transient — the server round-trip may not have completed
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Pending
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Denied, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then client {word} is available for the delegated entity
///
/// Polls until the named client observes EntityAuthStatus::Available.
/// Covers [entity-delegation-11.t1]: release returns Denied clients to Available.
#[then("client {word} is available for the delegated entity")]
fn then_client_is_available_for_delegated_entity(
    ctx: &TestWorldRef,
    name: String,
) -> AssertOutcome<()> {
    let client_key: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage(&name))
        .unwrap_or_else(|| panic!("No client '{}' has been connected", name));
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => AssertOutcome::Passed(()),
                // Releasing and Granted are transient on the way to Available
                Some(EntityAuthStatus::Releasing) | Some(EntityAuthStatus::Granted) => {
                    AssertOutcome::Pending
                }
                // Denied may still be present right after holder releases — wait for convergence
                Some(EntityAuthStatus::Denied) | Some(EntityAuthStatus::Requested) => {
                    AssertOutcome::Pending
                }
                None => AssertOutcome::Pending,
            }
        } else {
            // Entity may have briefly left scope during scope removal — keep waiting
            AssertOutcome::Pending
        }
    })
}

/// Step: Then the delegated entity is no longer in client A's world
///
/// Polls until client A no longer has the entity in its local world.
/// Covers [entity-delegation-13.t1]: entity leaves scope due to server exclude.
#[then("the delegated entity is no longer in client A's world")]
fn then_delegated_entity_is_no_longer_in_client_a_world(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_a, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Step: Then client A observes Delegated replication config for the entity
///
/// Asserts that client A can query the entity's replication_config and see
/// ReplicationConfig::Delegated. Covers [entity-delegation-17.t1].
#[then("client A observes Delegated replication config for the entity")]
fn then_client_a_observes_delegated_replication_config(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(ClientReplicationConfig::Delegated) => AssertOutcome::Passed(()),
                Some(other) => AssertOutcome::Failed(format!(
                    "expected Delegated replication config, got {:?}",
                    other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: And client A observes Available authority status for the entity
///
/// Asserts that client A observes EntityAuthStatus::Available (no holder).
/// Covers [entity-delegation-17.t1]: authority is observable via status.
#[then("client A observes Available authority status for the entity")]
fn then_client_a_observes_available_authority_status(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no delegated entity spawned");

    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => AssertOutcome::Passed(()),
                Some(other) => AssertOutcome::Failed(format!(
                    "expected Available authority status, got {:?}",
                    other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}
