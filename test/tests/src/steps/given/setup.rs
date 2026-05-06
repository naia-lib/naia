//! Given-step bindings: protocol, server, client, room initialization.
//!
//! This module owns the preconditions that put the system into a
//! "ready to act on" state: a running server, one or more connected
//! clients, joined rooms.

use naia_server::ServerConfig;
use naia_test_harness::protocol;
use namako_engine::given;

use crate::steps::world_helpers::connect_client;
use crate::TestWorldMut;

/// Given a server is running.
///
/// Initializes the scenario, starts a server with default config, and
/// creates a default room (stored as `last_room`). After this step
/// the test world has exactly one server, zero clients, one room.
#[given("a server is running")]
fn given_server_running(ctx: &mut TestWorldMut) {
    let scenario = ctx.init();
    scenario.server_start(ServerConfig::default(), protocol());
    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));
    scenario.set_last_room(room_key);
}

/// Given a client connects.
///
/// Mirror of the When variant — usable as `Given` (precondition) or
/// `And` after another Given. Drives the standard handshake via
/// [`connect_client`].
#[given("a client connects")]
fn given_client_connects(ctx: &mut TestWorldMut) {
    connect_client(ctx);
}

/// Given client {name} connects.
///
/// Connects a labeled client ("A", "B", ...) and stores the
/// resulting ClientKey under `client_key_storage(name)`. Used by
/// multi-client tests where bindings reference specific clients.
#[given("client {word} connects")]
fn given_client_named_connects(ctx: &mut TestWorldMut, name: String) {
    use crate::steps::world_helpers::connect_test_client;
    connect_test_client(ctx, &name);
}
