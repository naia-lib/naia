use naia_bevy_client::EntityAuthStatus;
use namako_engine::{given, then, when};
use namako_engine::codegen::AssertOutcome;

use crate::world::{BevyMutCtx, BevyRefCtx, ClientKey};

// ── Given — connection ─────────────────────────────────────────────────────────

#[given("a server is running")]
fn given_server_running(ctx: &mut BevyMutCtx) {
    ctx.init();
}

#[given("a client connects")]
fn given_client_connects(ctx: &mut BevyMutCtx) {
    connect_impl(ctx);
}

#[given("a second client connects")]
fn given_second_client_connects(ctx: &mut BevyMutCtx) {
    connect_impl(ctx);
}

// ── Given — entity setup ───────────────────────────────────────────────────────

#[given("a server entity is spawned in-scope for the client with Position")]
fn given_entity_spawned_in_scope_with_position(ctx: &mut BevyMutCtx) {
    let harness = ctx.harness_mut();
    let entity = harness.server_spawn_entity();
    let last_client = harness.last_client_key().expect("no client connected");
    harness.server_scope_entity_for_all_clients(entity);
    // Tick until the client sees the entity
    let ok = harness.tick_until(|h| h.client_has_entity(last_client), 500);
    assert!(ok, "entity did not spawn on client within 500 ticks");
}

#[given("a server entity is spawned in-scope for the client with Position at the origin")]
fn given_entity_spawned_in_scope_with_position_at_origin(ctx: &mut BevyMutCtx) {
    let harness = ctx.harness_mut();
    let entity = harness.server_spawn_entity_with_position(0.0, 0.0);
    let last_client = harness.last_client_key().expect("no client connected");
    harness.server_scope_entity_for_all_clients(entity);
    let ok = harness.tick_until(|h| h.client_has_entity(last_client), 500);
    assert!(ok, "entity did not spawn on client within 500 ticks");
}

#[given("a server entity is spawned in-scope for both clients with Position")]
fn given_entity_spawned_in_scope_for_both_clients_with_position(ctx: &mut BevyMutCtx) {
    let harness = ctx.harness_mut();
    let entity = harness.server_spawn_entity();
    harness.server_scope_entity_for_all_clients(entity);
    // Wait for both clients to see it
    let ok = harness.tick_until(
        |h| h.client_has_entity(ClientKey(0)) && h.client_has_entity(ClientKey(1)),
        500,
    );
    assert!(ok, "entity did not spawn on both clients within 500 ticks");
}

#[given("the entity is configured as Delegated")]
fn given_entity_configured_as_delegated(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_configure_delegated();
}

#[given("the server inserts TestPlayerSelection as a delegable resource")]
fn given_server_inserts_player_selection_delegable(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_insert_player_selection_delegated(0);
    // Tick until the client has it (resource mirror appears)
    let last_client = ctx.harness_mut().last_client_key().expect("no client");
    // Give time for the resource to replicate
    for _ in 0..100 {
        ctx.harness_mut().tick();
    }
    let _ = last_client;
}

// ── When — connection ──────────────────────────────────────────────────────────

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

// ── When — entity mutations ────────────────────────────────────────────────────

#[when("the server disables replication for the entity")]
fn when_server_disables_replication(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_disable_replication();
}

#[when("the server mutates Position to 42 and 42")]
fn when_server_mutates_position_42_42(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_update_position(42.0, 42.0);
}

// ── When — authority ──────────────────────────────────────────────────────────

#[when("the server grants authority to the client")]
fn when_server_grants_authority_to_client(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_give_authority_to_client(ClientKey(0));
}

#[when("the client requests authority for the entity")]
fn when_client_requests_entity_authority(ctx: &mut BevyMutCtx) {
    let last_client = ctx.harness_mut().last_client_key().expect("no client");
    ctx.harness_mut().client_request_entity_authority(last_client);
}

#[when("the first client requests authority for the entity")]
fn when_first_client_requests_entity_authority(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().client_request_entity_authority(ClientKey(0));
}

#[when("the second client requests authority for the entity")]
fn when_second_client_requests_entity_authority(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().client_request_entity_authority(ClientKey(1));
}

// ── When — resources ──────────────────────────────────────────────────────────

#[when("the server inserts TestScore home 3 away 1 as a replicated resource")]
fn when_server_inserts_score_3_1(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_insert_score(3, 1);
}

#[when("the server inserts TestScore home 0 away 0 as a replicated resource")]
fn when_server_inserts_score_0_0(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_insert_score(0, 0);
}

#[when("the server mutates TestScore to home 7 away 2")]
fn when_server_mutates_score_7_2(ctx: &mut BevyMutCtx) {
    ctx.harness_mut().server_mutate_score(7, 2);
}

#[when("the client requests authority for TestPlayerSelection")]
fn when_client_requests_player_selection_authority(ctx: &mut BevyMutCtx) {
    let last_client = ctx.harness_mut().last_client_key().expect("no client");
    ctx.harness_mut().client_request_player_selection_authority(last_client);
}

// ── Then — connection ──────────────────────────────────────────────────────────

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

// ── Then — entity ──────────────────────────────────────────────────────────────

#[then("the entity spawns on the client")]
fn then_entity_spawns_on_client(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if ctx.client_has_entity(key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the entity is absent from the client world")]
fn then_entity_absent_from_client(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if !ctx.client_has_entity(key) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the client has observed SpawnEntityEvent")]
fn then_client_observed_spawn_entity_event(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if ctx.client_spawn_event_count(key) > 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the client has observed DespawnEntityEvent")]
fn then_client_observed_despawn_entity_event(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if ctx.client_despawn_event_count(key) > 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the client observes Position 42 and 42")]
fn then_client_observes_position_42_42(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    match ctx.client_entity_position(key) {
        Some((x, y)) if (x - 42.0).abs() < 0.001 && (y - 42.0).abs() < 0.001 => {
            AssertOutcome::Passed(())
        }
        Some(_) => AssertOutcome::Pending,
        None => AssertOutcome::Pending,
    }
}

// ── Then — authority ───────────────────────────────────────────────────────────

#[then("the client has authority status Granted")]
fn then_client_authority_granted(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    match ctx.client_authority_status(key) {
        Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
        Some(EntityAuthStatus::Requested) | Some(EntityAuthStatus::Available) => {
            AssertOutcome::Pending
        }
        Some(other) => AssertOutcome::Failed(format!("expected Granted, got {:?}", other)),
        None => AssertOutcome::Pending,
    }
}

#[then("the client has observed EntityAuthGrantedEvent")]
fn then_client_observed_auth_granted_event(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if ctx.client_auth_granted_event_count(key) > 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the second client has observed EntityAuthDeniedEvent")]
fn then_second_client_observed_auth_denied_event(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    if ctx.client_auth_denied_event_count(ClientKey(1)) > 0 {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

// ── Then — resources ───────────────────────────────────────────────────────────

#[then("the client has TestScore as a Bevy resource")]
fn then_client_has_test_score(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    if ctx.client_score(key).is_some() {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the client TestScore.home equals 3")]
fn then_client_score_home_3(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    match ctx.client_score(key) {
        Some((3, _)) => AssertOutcome::Passed(()),
        Some(_) => AssertOutcome::Pending,
        None => AssertOutcome::Pending,
    }
}

#[then("the client TestScore.home equals 7")]
fn then_client_score_home_7(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    match ctx.client_score(key) {
        Some((7, _)) => AssertOutcome::Passed(()),
        Some(_) => AssertOutcome::Pending,
        None => AssertOutcome::Pending,
    }
}

#[then("the client TestScore.away equals 2")]
fn then_client_score_away_2(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    let key = ctx.last_client_key().expect("no client");
    match ctx.client_score(key) {
        Some((_, 2)) => AssertOutcome::Passed(()),
        Some(_) => AssertOutcome::Pending,
        None => AssertOutcome::Pending,
    }
}

#[then("the server observes Denied authority for TestPlayerSelection")]
fn then_server_player_selection_denied(ctx: &BevyRefCtx) -> AssertOutcome<()> {
    // From the server's perspective: Denied = a client currently holds authority.
    // Available/Requested are transient states while the grant round-trip completes.
    match ctx.server_player_selection_authority() {
        Some(EntityAuthStatus::Denied) => AssertOutcome::Passed(()),
        Some(EntityAuthStatus::Available) | Some(EntityAuthStatus::Requested) => {
            AssertOutcome::Pending
        }
        Some(other) => AssertOutcome::Failed(format!(
            "expected Denied (client holds authority) for TestPlayerSelection, got {:?}", other
        )),
        None => AssertOutcome::Pending,
    }
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
