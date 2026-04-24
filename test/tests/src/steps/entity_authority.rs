//! Step bindings for Entity Authority contract (11_entity_authority.feature)
//!
//! These steps cover:
//!   - Non-delegated entity has no authority status (entity-authority-01)
//!   - Server holding authority puts all clients in Denied (entity-authority-09)
//!   - Server reset transitions all clients to Available (entity-authority-10)
//!   - Client release Granted → Available (entity-authority-06)
//!   - Authority granted/denied/reset events observable via client Events API (entity-authority-16)
//!   - Error returns for non-delegated entity authority request (entity-authority-07)

use naia_server::ReplicationConfig as ServerReplicationConfig;
use naia_test_harness::{ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, EntityKey, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";

fn client_key_storage(name: &str) -> String {
    format!("client_{}", name)
}

// ============================================================================
// Given Steps — Non-delegated Entity Setup
// ============================================================================

/// Step: And the server spawns a non-delegated entity in-scope for client A
///
/// Spawns a server-owned entity with `ReplicationConfig::Public` (not Delegated),
/// enters it in the room, includes it in client A's scope, and waits until
/// client A has the entity replicated locally.
#[given("the server spawns a non-delegated entity in-scope for client A")]
fn given_server_spawns_non_delegated_entity_in_scope_for_client_a(ctx: &mut TestWorldMut) {
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
                    // Explicitly Public — NOT Delegated
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

// ============================================================================
// Then Steps — Authority Status Assertions
// ============================================================================

/// Step: Then client A observes no authority status for the entity
///
/// Asserts that authority() returns None for a non-delegated entity.
/// Covers [entity-authority-01.t1]: authority is None for non-delegated.
#[then("client A observes no authority status for the entity")]
fn then_client_a_observes_no_authority_status(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                None => AssertOutcome::Passed(()),
                Some(status) => AssertOutcome::Failed(format!(
                    "expected None authority for non-delegated entity, got {:?}",
                    status
                )),
            }
        } else {
            // Entity not yet replicated; keep waiting
            AssertOutcome::Pending
        }
    })
}

/// Step: Then client A receives an authority granted event for the entity
///
/// Polls until the last client A sees a `ClientEntityAuthGrantedEvent` for the
/// last entity. Covers [entity-authority-16.t1]: authority grant observable via
/// the client Events API.
#[then("client A receives an authority granted event for the entity")]
fn then_client_a_receives_authority_granted_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_a, |c| {
        if let Some(ek) = c.read_event::<ClientEntityAuthGrantedEvent>() {
            if ek == entity_key {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then client A receives an authority reset event for the entity
///
/// Polls until client A sees a `ClientEntityAuthResetEvent` for the last entity.
/// Covers [entity-authority-16.t1]: authority reset observable via client Events API.
/// Reset fires when authority returns to Available (e.g., server releases).
#[then("client A receives an authority reset event for the entity")]
fn then_client_a_receives_authority_reset_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_a: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_a, |c| {
        if let Some(ek) = c.read_event::<ClientEntityAuthResetEvent>() {
            if ek == entity_key {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Step: Then client B receives an authority denied event for the entity
///
/// Polls until client B sees a `ClientEntityAuthDeniedEvent` for the last entity.
/// Covers [entity-authority-16.t1]: authority denied is observable via client Events API.
/// Denied event fires when status transitions Requested → Denied.
#[then("client B receives an authority denied event for the entity")]
fn then_client_b_receives_authority_denied_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_b: ClientKey = ctx
        .scenario()
        .bdd_get(&client_key_storage("B"))
        .expect("client B not connected");
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    ctx.client(client_b, |c| {
        if let Some(ek) = c.read_event::<ClientEntityAuthDeniedEvent>() {
            if ek == entity_key {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ============================================================================
// When Steps — Authority Requests on Non-Delegated Entities
// ============================================================================

const LAST_REQUEST_ERROR: &str = "last_request_error";

/// Step: When client A requests authority for the non-delegated entity
///
/// Calls request_authority() on the last non-delegated entity. Does NOT panic
/// on error — stores whether the call returned an error for assertion.
/// Covers [entity-authority-07.t1]: request on non-delegated MUST return error.
#[when("client A requests authority for the non-delegated entity")]
fn when_client_a_requests_authority_non_delegated(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    let client_a: ClientKey = scenario
        .bdd_get(&client_key_storage("A"))
        .expect("client A not connected");
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");

    let returned_error = scenario.mutate(|mctx| {
        let mut returned_error = false;
        mctx.client(client_a, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                returned_error = entity.request_authority().is_err();
            }
        });
        returned_error
    });

    scenario.bdd_store(LAST_REQUEST_ERROR, returned_error);
}

/// Step: Then the authority request fails with an error
///
/// Checks that the stored result from the last authority request was an error.
/// Covers [entity-authority-07.t1]: non-delegated entity request MUST return error.
#[then("the authority request fails with an error")]
fn then_authority_request_fails_with_error(ctx: &TestWorldRef) -> AssertOutcome<()> {
    match ctx.scenario().bdd_get::<bool>(LAST_REQUEST_ERROR) {
        Some(true) => AssertOutcome::Passed(()),
        Some(false) => AssertOutcome::Failed(
            "expected request_authority to return Err for non-delegated entity, got Ok".to_string(),
        ),
        None => AssertOutcome::Failed("no request result stored".to_string()),
    }
}
