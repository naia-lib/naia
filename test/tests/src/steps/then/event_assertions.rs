//! Then-step bindings: event-history predicates.
//!
//! Event assertions check that the system *emitted* a specific event
//! (or sequence of events). Distinct from
//! [`state_assertions`](super::state_assertions) which check current
//! observable state.

use naia_test_harness::{
    ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent,
    ClientKey, ClientSpawnEntityEvent, EntityKey, ServerEntityAuthGrantEvent,
    ServerEntityAuthResetEvent, ServerPublishEntityEvent,
};

use crate::steps::prelude::*;

// ──────────────────────────────────────────────────────────────────────
// Client-side entity-lifecycle events
// ──────────────────────────────────────────────────────────────────────

/// Then the client receives a spawn event for the entity.
///
/// Polls until the last client surfaces a `ClientSpawnEntityEvent` for
/// the stored entity. Covers [client-events-04.t1] and
/// [client-events-09.t1] (scope re-enter emits Spawn).
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

/// Then the client receives a despawn event for the entity.
///
/// `ClientDespawnEntityEvent` is consumed before step closures run,
/// so entity-absence is the correct observable proxy. Covers
/// [client-events-09.t1].
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

/// Then the client receives a component update event for the entity.
///
/// Covers [client-events-07.t1] (one-shot per applied change).
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

/// Then the client receives a component insert event for the entity.
///
/// Covers [client-events-06.t1] (insert events fire for in-scope
/// component additions).
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

/// Then the client receives a component remove event for the entity.
///
/// Covers [client-events-08.t1] (one-shot per applied removal).
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

// ──────────────────────────────────────────────────────────────────────
// Client-side authority events
// ──────────────────────────────────────────────────────────────────────

/// Then client A receives an authority granted event for the entity.
///
/// Covers [entity-authority-16.t1] (auth grant observable via Events API).
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

/// Then client A receives an authority reset event for the entity.
///
/// Reset fires when authority returns to Available (e.g. server release).
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

/// Then client B receives an authority denied event for the entity.
///
/// Denied fires when status transitions Requested → Denied.
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

// ──────────────────────────────────────────────────────────────────────
// Server-side events
// ──────────────────────────────────────────────────────────────────────

/// Then the server observes a spawn event for client A.
///
/// `ServerSpawnEntityEvent` only fires for client-spawned entities;
/// for server-owned entities scope membership is the proxy. Covers
/// [server-events-07.t1].
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

/// Then the server observes an authority grant event for client A.
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

/// Then the server observes an authority reset event.
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

/// Then the server observes a publish event for client A.
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

// ──────────────────────────────────────────────────────────────────────
// Connection lifecycle — connect / disconnect / reject events
// ──────────────────────────────────────────────────────────────────────

/// Then the connection is rejected with ProtocolMismatch.
#[then("the connection is rejected with ProtocolMismatch")]
fn then_connection_rejected_protocol_mismatch(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::{ClientRejectEvent, RejectReason};
    let client_key = ctx.last_client();
    ctx.client(client_key, |client| {
        if let Some(reason) = client.read_event::<ClientRejectEvent>() {
            if reason == RejectReason::ProtocolMismatch {
                return AssertOutcome::Passed(());
            }
        }
        AssertOutcome::Pending
    })
}

/// Then the client observes ConnectEvent.
#[then("the client observes ConnectEvent")]
fn then_client_observes_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if ctx.client_observed(client_key, TrackedClientEvent::Connect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client observes RejectEvent.
#[then("the client observes RejectEvent")]
fn then_client_observes_reject(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if ctx.client_observed(client_key, TrackedClientEvent::Reject) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client does not observe ConnectEvent.
#[then("the client does not observe ConnectEvent")]
fn then_client_no_connect(ctx: &TestWorldRef) {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    assert!(
        !ctx.client_observed(client_key, TrackedClientEvent::Connect),
        "Client should NOT have observed ConnectEvent but did. History: {:?}",
        ctx.client_event_history(client_key)
    );
}

/// Then the client does not observe DisconnectEvent.
#[then("the client does not observe DisconnectEvent")]
fn then_client_no_disconnect(ctx: &TestWorldRef) {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    assert!(
        !ctx.client_observed(client_key, TrackedClientEvent::Disconnect),
        "Client should NOT have observed DisconnectEvent but did. History: {:?}",
        ctx.client_event_history(client_key)
    );
}

/// Then the server has observed ConnectEvent.
#[then("the server has observed ConnectEvent")]
fn then_server_has_observed_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedServerEvent;
    if ctx.server_observed(TrackedServerEvent::Connect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client has observed ConnectEvent.
#[then("the client has observed ConnectEvent")]
fn then_client_has_observed_connect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if ctx.client_observed(client_key, TrackedClientEvent::Connect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the server has observed DisconnectEvent.
#[then("the server has observed DisconnectEvent")]
fn then_server_has_observed_disconnect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedServerEvent;
    if ctx.server_observed(TrackedServerEvent::Disconnect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client has observed DisconnectEvent.
#[then("the client has observed DisconnectEvent")]
fn then_client_has_observed_disconnect(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TrackedClientEvent;
    let client_key = ctx.last_client();
    if ctx.client_observed(client_key, TrackedClientEvent::Disconnect) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the server observes a despawn event for client A.
///
/// Scope-absence proxy (mirrors spawn-event proxy above). Covers
/// [server-events-09.t1].
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
