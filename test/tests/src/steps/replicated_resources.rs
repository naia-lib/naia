//! Step bindings for the Replicated Resources feature.
//!
//! Source: `test/specs/features/21_replicated_resources.feature`
//!
//! These bindings cover the harness-bindable scenarios (Rules 01–08
//! minus the Bevy-app + wire-inspection cases, which are tagged
//! `@Deferred` in the spec because they require Bevy-app integration
//! tests not yet in this harness).
//!
//! The harness uses `naia_demo_world` directly (not Bevy), so we drive
//! the Replicated Resources via the harness's `ServerMutateCtx` /
//! `ClientMutateCtx` resource methods (which forward to
//! `naia_server::Server::insert_resource` etc.). The Mode B mirror
//! systems and `Res<R>` user surface are tested in the Bevy-app
//! integration tests under `adapters/bevy/{server,client}/tests/`.

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType};
use naia_server::ServerConfig;
use naia_shared::EntityAuthStatus;
use naia_test_harness::{
    protocol, Auth, ClientKey, ServerAuthEvent, ServerConnectEvent, TestPlayerSelection, TestScore,
};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

const ALICE: &str = "alice";
const ALICE_KEY_SLOT: &str = "client_alice";

fn test_client_config() -> ClientConfig {
    let mut config = ClientConfig::default();
    config.send_handshake_interval = Duration::from_millis(0);
    config.jitter_buffer = JitterBufferType::Bypass;
    config
}

fn ensure_server_started(ctx: &mut TestWorldMut) {
    if !ctx.is_initialized() {
        let scenario = ctx.init();
        let p = protocol();
        scenario.server_start(ServerConfig::default(), p);
        let room_key = scenario.mutate(|c| c.server(|server| server.make_room().key()));
        scenario.set_last_room(room_key);
    }
}

fn connect_client(ctx: &mut TestWorldMut, name: &str, slot: &str) -> ClientKey {
    let scenario = ctx.scenario_mut();
    let p = protocol();
    let auth = Auth::new(name, "secret");
    let client_key = scenario.client_start(name, auth.clone(), test_client_config(), p);

    scenario.expect(|c| {
        c.server(|server| {
            server
                .read_event::<ServerAuthEvent<Auth>>()
                .filter(|(k, _)| *k == client_key)
                .map(|_| ())
        })
    });
    scenario.mutate(|c| c.server(|server| server.accept_connection(&client_key)));
    scenario.expect(|c| {
        c.server(|server| server.read_event::<ServerConnectEvent>().map(|_| ()))
    });
    let room_key = scenario.last_room();
    scenario.mutate(|c| {
        c.server(|server| {
            server
                .room_mut(&room_key)
                .expect("room exists")
                .add_user(&client_key);
        });
    });
    scenario.expect(|c| {
        let connected = c.client(client_key, |cl| cl.connection_status().is_connected());
        let user_exists = c.server(|s| s.user_exists(&client_key));
        (connected && user_exists).then_some(())
    });
    scenario.bdd_store(slot, client_key);
    client_key
}

fn last_client_or(ctx: &mut TestWorldMut, slot: &str) -> ClientKey {
    let scenario = ctx.scenario_mut();
    scenario
        .bdd_get::<ClientKey>(slot)
        .unwrap_or_else(|| scenario.last_client())
}

// =============================================================================
// Given — Protocol setup
// =============================================================================

/// All harness scenarios use the same `naia_test_harness::protocol()`,
/// which already registers TestScore / TestMatchState / TestPlayerSelection
/// as resources. The "Given a Naia protocol with replicated resource type X"
/// steps are therefore identity assertions against the protocol — we just
/// verify the kind is registered.

#[given(r#"a Naia protocol with replicated resource type "Score""#)]
fn given_protocol_with_score(_ctx: &mut TestWorldMut) {
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestScore>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestScore must be registered as a resource"
    );
}

#[given(r#"a Naia protocol with delegable replicated resource type "PlayerSelection""#)]
fn given_protocol_with_player_selection(_ctx: &mut TestWorldMut) {
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestPlayerSelection>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestPlayerSelection must be registered as a resource"
    );
}

// =============================================================================
// Given — Server + client lifecycle
// =============================================================================

#[given("a server and one connected client")]
fn given_server_and_one_client(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
    let _ = connect_client(ctx, ALICE, ALICE_KEY_SLOT);
}

#[given(r#"a server with PlayerSelection \{ selected_id: 0 \} and connected client "alice""#)]
fn given_server_with_player_selection_and_alice(ctx: &mut TestWorldMut) {
    ensure_server_started(ctx);
    let client_key = connect_client(ctx, ALICE, ALICE_KEY_SLOT);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_resource(TestPlayerSelection::new(0)),
                "insert PlayerSelection should succeed for fresh type"
            );
        });
    });
    // Settle: ensure replication reaches the client.
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
    let _ = client_key;
}

#[given("the initial replication round trip has elapsed")]
fn given_initial_round_trip_elapsed(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..20 {
        scenario.mutate(|_| {});
    }
}

// =============================================================================
// When — Insertions, mutations, replication ticks
// =============================================================================

#[when(r#"the server inserts Score \{ home: 0, away: 0 \} as a dynamic resource"#)]
fn when_server_inserts_score_dynamic(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_resource(TestScore::new(0, 0)),
                "insert Score should succeed"
            );
        });
    });
}

#[when("one full replication round trip elapses")]
fn when_one_full_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

#[when("one replication round trip elapses")]
fn when_one_round_trip(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

#[when("alice requests authority on PlayerSelection")]
fn when_alice_requests_authority(ctx: &mut TestWorldMut) {
    let client_key = last_client_or(ctx, ALICE_KEY_SLOT);
    let scenario = ctx.scenario_mut();
    // Configure delegation server-side (the Given step inserted the
    // resource as plain server-authoritative; this When activates
    // delegation so the client can request authority).
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(server.configure_resource::<TestPlayerSelection>(
                naia_server::ReplicationConfig::delegated()
            ));
        });
    });
    // Wait for the client's view to reach Available (EnableDelegation
    // propagates over the reliable channel).
    scenario.expect(|c| {
        c.client(client_key, |cl| {
            (cl.resource_authority_status::<TestPlayerSelection>()
                == Some(EntityAuthStatus::Available))
            .then_some(())
        })
    });
    scenario.mutate(|c| {
        c.client(client_key, |cl| {
            let res = cl.request_resource_authority::<TestPlayerSelection>();
            assert!(res.is_ok(), "request_resource_authority: {:?}", res);
        });
    });
}

// =============================================================================
// Then — Assertions
// =============================================================================

#[then("the client's Score is present")]
fn then_client_has_score(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let key = ctx.last_client();
    if ctx.client(key, |c| c.has_resource::<TestScore>()) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

#[then("the client's Score.home equals 0")]
fn then_client_score_home_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.home)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

#[then("the client's Score.away equals 0")]
fn then_client_score_away_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.away)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

#[then(r#"alice's authority status for PlayerSelection is "Granted""#)]
fn then_alice_auth_granted(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource_authority_status::<TestPlayerSelection>()) {
        Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}
