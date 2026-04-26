use namako_engine::{given, then, when};

use crate::world::{BevyMutCtx, BevyRefCtx};

// ── Given ──────────────────────────────────────────────────────────────────────

#[given("a server is running")]
fn given_server_running(ctx: &mut BevyMutCtx) {
    ctx.init();
}

#[given("a client connects")]
fn given_client_connects(ctx: &mut BevyMutCtx) {
    connect_impl(ctx);
}

// ── When ───────────────────────────────────────────────────────────────────────

#[when("a client connects")]
fn when_client_connects(ctx: &mut BevyMutCtx) {
    connect_impl(ctx);
}

#[when("the server disconnects the client")]
fn when_server_disconnects(ctx: &mut BevyMutCtx) {
    let harness = ctx.harness_mut();
    let last_key = harness.last_client_key().expect("no clients connected");
    harness.disconnect_last_user();

    let ok = harness.tick_until(|h| h.client_disconnect_count(last_key) > 0, 500);
    assert!(ok, "client did not observe disconnect within 500 ticks");
}

// ── Then ───────────────────────────────────────────────────────────────────────

#[then("the server has {int} connected client(s)")]
fn then_server_has_clients(ctx: &BevyRefCtx, expected: usize) {
    assert_eq!(
        ctx.server_connected_count(),
        expected,
        "server should have {} connected clients",
        expected
    );
}

#[then("the client is connected")]
fn then_client_connected(ctx: &BevyRefCtx) {
    let key = ctx.last_client_key().expect("no client");
    assert!(ctx.client_is_connected(key), "client should be connected");
}

#[then("the client is not connected")]
fn then_client_not_connected(ctx: &BevyRefCtx) {
    let key = ctx.last_client_key().expect("no client");
    assert!(!ctx.client_is_connected(key), "client should not be connected");
}

#[then("the server has observed ConnectEvent")]
fn then_server_observed_connect(ctx: &BevyRefCtx) {
    assert!(ctx.server_connect_count() > 0, "server should have observed ConnectEvent");
}

#[then("the server has observed DisconnectEvent")]
fn then_server_observed_disconnect(ctx: &BevyRefCtx) {
    assert!(ctx.server_disconnect_count() > 0, "server should have observed DisconnectEvent");
}

#[then("the client has observed ConnectEvent")]
fn then_client_observed_connect(ctx: &BevyRefCtx) {
    let key = ctx.last_client_key().expect("no client");
    assert!(ctx.client_connect_count(key) > 0, "client should have observed ConnectEvent");
}

#[then("the client has observed DisconnectEvent")]
fn then_client_observed_disconnect(ctx: &BevyRefCtx) {
    let key = ctx.last_client_key().expect("no client");
    assert!(ctx.client_disconnect_count(key) > 0, "client should have observed DisconnectEvent");
}

#[then("the server observed ConnectEvent before DisconnectEvent")]
fn then_server_connect_before_disconnect(ctx: &BevyRefCtx) {
    assert!(ctx.server_connect_count() > 0, "server needs ConnectEvent");
    assert!(ctx.server_disconnect_count() > 0, "server needs DisconnectEvent");
}

#[then("the client observed ConnectEvent before DisconnectEvent")]
fn then_client_connect_before_disconnect(ctx: &BevyRefCtx) {
    let key = ctx.last_client_key().expect("no client");
    assert!(ctx.client_connect_count(key) > 0, "client needs ConnectEvent");
    assert!(ctx.client_disconnect_count(key) > 0, "client needs DisconnectEvent");
}

// ── Internal helpers ───────────────────────────────────────────────────────────

fn connect_impl(ctx: &mut BevyMutCtx) {
    let harness = ctx.harness_mut();
    let prev_connects = harness.server_connect_count();
    let client_key = harness.add_client();

    let ok = harness.tick_until(
        |h| h.server_connect_count() > prev_connects && h.client_connect_count(client_key) > 0,
        500,
    );
    assert!(ok, "client did not connect within 500 ticks");
}
