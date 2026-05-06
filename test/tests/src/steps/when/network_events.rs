//! When-step bindings: connection/disconnection/tick-passage events.
//!
//! Network events are *imperative*: a client connects, the server
//! disconnects somebody, N ticks elapse. They drive the system into a
//! new observable state without modeling a domain action.

use naia_test_harness::{ClientDisconnectEvent, EntityKey, TrackedClientEvent, TrackedServerEvent};
use namako_engine::when;

use crate::steps::world_helpers::{
    connect_client, connect_named_client, LAST_ENTITY_KEY, SECOND_CLIENT_KEY,
};
use crate::TestWorldMut;

/// When a client connects.
///
/// Mirror of the Given variant — usable as `When` (the action under
/// test) or `And` after another When. Drives the standard handshake
/// via [`connect_client`].
#[when("a client connects")]
fn when_client_connects(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// When the server disconnects the client.
///
/// Initiates a server-side disconnect of the most-recently-connected
/// client and waits for the client to observe the
/// `ClientDisconnectEvent`. Tracks both the server-side and
/// client-side events so subsequent Then steps can assert on them.
#[when("the server disconnects the client")]
fn when_server_disconnects(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.disconnect_user(&client_key);
        });
    });
    scenario.track_server_event(TrackedServerEvent::Disconnect);

    scenario.expect(|ctx| {
        ctx.client(client_key, |client| client.read_event::<ClientDisconnectEvent>())
    });
    scenario.track_client_event(client_key, TrackedClientEvent::Disconnect);

    scenario.allow_flexible_next();
}

/// When a second client connects and the entity enters scope for it.
///
/// Used by world-integration late-join tests. Connects a second client
/// via the standard handshake and includes the stored entity in its
/// scope as part of the room-add step. Stores the new client key
/// under `SECOND_CLIENT_KEY` for downstream Then steps.
#[when("a second client connects and the entity enters scope for it")]
fn when_second_client_connects_and_entity_enters_scope(ctx: &mut TestWorldMut) {
    let entity_key: EntityKey = ctx
        .scenario_mut()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("no entity spawned for world integration test");

    let client_key = connect_named_client(
        ctx,
        "SecondClient",
        "second_client",
        Some(Box::new(move |scenario, ck| {
            scenario.mutate(|mctx| {
                mctx.server(|server| {
                    if let Some(mut scope) = server.user_scope_mut(&ck) {
                        scope.include(&entity_key);
                    }
                });
            });
        })),
    );

    ctx.scenario_mut().bdd_store(SECOND_CLIENT_KEY, client_key);
}
