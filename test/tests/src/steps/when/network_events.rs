//! When-step bindings: connection/disconnection/tick-passage events.
//!
//! Network events are *imperative*: a client connects, the server
//! disconnects somebody, N ticks elapse. They drive the system into a
//! new observable state without modeling a domain action.

use naia_test_harness::{ClientDisconnectEvent, TrackedClientEvent, TrackedServerEvent};
use namako_engine::when;

use crate::steps::world_helpers::connect_client;
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
