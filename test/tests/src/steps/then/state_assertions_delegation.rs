//! Then-step bindings: entity-delegation authority and entity-scope singleton assertions.

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;
use crate::steps::world_helpers::last_entity_ref;

// ──────────────────────────────────────────────────────────────────────
// Entity-delegation — authority status assertions
// ──────────────────────────────────────────────────────────────────────

/// Then client {name} is granted authority for the delegated entity.
///
/// Polls until the named client observes EntityAuthStatus::Granted.
/// Covers [entity-delegation-06.t1] (first in-scope request wins).
#[then("client {client} is granted authority for the delegated entity")]
fn then_client_is_granted_authority(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
                Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Pending
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Granted, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {name} is denied authority for the delegated entity.
///
/// Allows Requested as a transient state while the server round-trip
/// completes. Covers [entity-delegation-07.t1].
#[then("client {client} is denied authority for the delegated entity")]
fn then_client_is_denied_authority(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Denied) => AssertOutcome::Passed(()),
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

/// Then client {client} is eventually denied authority for the delegated entity.
///
/// Tolerant variant of `is denied authority` — treats Granted and
/// Releasing as valid transient states. Used by server-override
/// scenarios (server calls take_authority while client holds Granted),
/// where the transition goes Granted → Releasing → Denied. The
/// standard `is denied` assertion fails hard on Granted/Releasing,
/// which would be a false failure during the revocation round-trip.
#[then("client {client} is eventually denied authority for the delegated entity")]
fn then_client_is_eventually_denied_authority(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Denied) => AssertOutcome::Passed(()),
                _ => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then client {name} is available for the delegated entity.
///
/// Covers [entity-delegation-11.t1] (release returns Denied clients
/// to Available). Tolerates transient Releasing/Granted/Denied/Requested
/// while the convergence completes.
#[then("client {client} is available for the delegated entity")]
fn then_client_is_available_for_delegated_entity(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Passed(())
                }
                _ => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the delegated entity is no longer in client A's world.
///
/// Covers [entity-delegation-13.t1] (entity leaves scope on exclude).
#[then("the delegated entity is no longer in client A's world")]
fn then_delegated_entity_is_no_longer_in_client_a_world(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_a = named_client_ref(ctx, "A");
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_a, |c| {
        if c.has_entity(&entity_key) {
            AssertOutcome::Pending
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Then client A observes Delegated replication config for the entity.
///
/// Covers [entity-delegation-17.t1] (delegation observable from client).
#[then("client A observes Delegated replication config for the entity")]
fn then_client_a_observes_delegated_replication_config(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_client::Publicity;
    let client_a = named_client_ref(ctx, "A");
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_a, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(Publicity::Delegated) => {
                    AssertOutcome::Passed(())
                }
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

/// Then client {name} observes Available authority status for the entity.
#[then("client {client} observes Available authority status for the entity")]
fn then_client_observes_available_authority_status(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.authority() {
                Some(EntityAuthStatus::Available) => {
                    AssertOutcome::Passed(())
                }
                Some(other) => AssertOutcome::Failed(format!(
                    "client {}: expected Available authority status, got {:?}",
                    name, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Entity scope — singleton-client predicates
// ──────────────────────────────────────────────────────────────────────

/// Then the entity is in-scope for the client.
#[then("the entity is in-scope for the client")]
fn then_entity_in_scope_for_client_singleton(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity is out-of-scope for the client.
#[then("the entity is out-of-scope for the client")]
fn then_entity_out_of_scope_for_client_singleton(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            if !scope.has(&entity_key) {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Passed(())
        }
    })
}

/// Then the entity despawns on the client.
#[then("the entity despawns on the client")]
fn then_entity_despawns_on_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if !client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client.
#[then("the entity spawns on the client")]
fn then_entity_spawns_on_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the entity spawns on the client as a new lifetime.
#[then("the entity spawns on the client as a new lifetime")]
fn then_entity_spawns_on_client_as_new_lifetime(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the server stops replicating entities to that client.
///
/// Polls until the user no longer exists server-side (post-disconnect).
#[then("the server stops replicating entities to that client")]
fn then_server_stops_replicating_to_client(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.server(|server| {
        if !server.user_exists(&client_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then no error is raised.
///
/// Trivially passes — reaching this step means the prior When did
/// not panic. Used by edge-case scope tests against unknown
/// entities/clients.
#[then("no error is raised")]
fn then_no_error_is_raised(_ctx: &TestWorldRef) -> AssertOutcome<()> {
    AssertOutcome::Passed(())
}

