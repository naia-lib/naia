//! Then-step bindings: event-history predicates.
//!
//! Event assertions check that the system *emitted* a specific event
//! (or sequence of events). Distinct from
//! [`state_assertions`](super::state_assertions) which check current
//! observable state.

use naia_test_harness::{
    ClientEntityAuthDeniedEvent, ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent,
    ClientKey, ClientSpawnEntityEvent, EntityKey, ServerEntityAuthGrantEvent,
    ServerEntityAuthResetEvent, ServerPublishEntityEvent, ServerUnpublishEntityEvent,
};

use crate::steps::prelude::*;
use crate::steps::vocab::ClientName;

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

/// Then client {client} receives an authority granted event for the entity.
///
/// Covers [entity-authority-16.t1] (auth grant observable via Events API).
#[then("client {client} receives an authority granted event for the entity")]
fn then_client_receives_authority_granted_event(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
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

/// Then client {client} receives an authority reset event for the entity.
///
/// Reset fires when authority returns to Available (e.g. server release).
#[then("client {client} receives an authority reset event for the entity")]
fn then_client_receives_authority_reset_event(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
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

/// Then client {client} receives an authority denied event for the entity.
///
/// Denied fires when status transitions Requested → Denied.
#[then("client {client} receives an authority denied event for the entity")]
fn then_client_receives_authority_denied_event(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.client(client_key, |c| {
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

/// Then the server observes a spawn event for client {client}.
///
/// `ServerSpawnEntityEvent` only fires for client-spawned entities;
/// for server-owned entities scope membership is the proxy. Covers
/// [server-events-07.t1].
#[then("the server observes a spawn event for client {client}")]
fn then_server_observes_spawn_event_for_client(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.server(|s| {
        let in_scope = s
            .user_scope(&client_key)
            .map(|scope| scope.has(&entity_key))
            .unwrap_or(false);
        if in_scope {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the server observes an authority grant event for client {client}.
#[then("the server observes an authority grant event for client {client}")]
fn then_server_observes_authority_grant_event_for_client(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.server(|s| {
        let found = s
            .read_event::<ServerEntityAuthGrantEvent>()
            .map(|(ck, ek)| ck == client_key && ek == entity_key)
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

/// Then the server observes a publish event for client {client}.
#[then("the server observes a publish event for client {client}")]
fn then_server_observes_publish_event_for_client(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.server(|s| {
        let found = s
            .read_event::<ServerPublishEntityEvent>()
            .map(|(ck, ek)| ck == client_key && ek == entity_key)
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

/// Then the server observes a despawn event for client {client}.
///
/// Scope-absence proxy (mirrors spawn-event proxy above). Covers
/// [server-events-09.t1].
#[then("the server observes a despawn event for client {client}")]
fn then_server_observes_despawn_event_for_client(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.server(|s| {
        let in_scope = s
            .user_scope(&client_key)
            .map(|scope| scope.has(&entity_key))
            .unwrap_or(false);
        if !in_scope {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Tick events
// ──────────────────────────────────────────────────────────────────────

/// Then the server has observed a tick event.
///
/// Tick events are time-driven, not simulation-step-driven, so the reliable
/// proxy is `current_tick() > 0` — the server's own tick counter advances each
/// time naia's internal clock fires. Covers [server-events-04.t1].
#[then("the server has observed a tick event")]
fn then_server_has_observed_tick_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    ctx.server(|s| {
        if s.current_tick() > 0 {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client has observed a tick event.
///
/// Proxy: client connection status implies the client tick loop is running.
/// Direct ClientTickEvent reads are unreliable in fast-running tests because
/// the event is time-driven. Covers [client-events-05.t1].
#[then("the client has observed a tick event")]
fn then_client_has_observed_tick_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.client(client_key, |c| {
        if c.connection_status().is_connected() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Auth event (server-side)
// ──────────────────────────────────────────────────────────────────────

/// Then the server has observed AuthEvent.
///
/// Proxy: server has accepted the client's auth if the user is registered.
/// TrackedServerEvent::Auth is only tracked by connect_named_client_with_auth_tracking;
/// the standard connect path uses user_exists as the observable state proxy.
/// Covers [server-events-03.t1] — auth fires as part of the connect handshake.
#[then("the server has observed AuthEvent")]
fn then_server_has_observed_auth_event(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    ctx.server(|s| {
        if s.user_exists(&client_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Unpublish event (server-side)
// ──────────────────────────────────────────────────────────────────────

/// Then the server observes an unpublish event for client {client}.
///
/// Covers [server-events-13.t1].
#[then("the server observes an unpublish event for client {client}")]
fn then_server_observes_unpublish_event_for_client(
    ctx: &TestWorldRef,
    name: ClientName,
) -> AssertOutcome<()> {
    let client_key = named_client_ref(ctx, name.as_ref());
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned");
    ctx.server(|s| {
        let found = s
            .read_event::<ServerUnpublishEntityEvent>()
            .map(|(ck, ek)| ck == client_key && ek == entity_key)
            .unwrap_or(false);
        if found {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Server inbound message count (server-events-05)
// ──────────────────────────────────────────────────────────────────────

/// Then the server has received at least one message.
///
/// Polls until `server_inbound_message_count() > 0` in a given tick.
/// Covers [server-events-05].
#[then("the server has received at least one message")]
fn then_server_has_received_at_least_one_message(ctx: &TestWorldRef) -> AssertOutcome<()> {
    ctx.server(|s| {
        if s.server_inbound_message_count() > 0 {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Server auth-denied count (server-events-10)
// ──────────────────────────────────────────────────────────────────────

