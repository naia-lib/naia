//! Given-step bindings: replicated resources preconditions.
//!
//! Split out of `given/state.rs` (Q3, 2026-05-07). See `state_*` siblings
//! and `world_helpers` for cross-cutting helpers.

use crate::steps::prelude::*;

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — protocol assertions + initial-state setup
// ──────────────────────────────────────────────────────────────────────

/// Given a Naia protocol with replicated resource type "Score".
///
/// Identity assertion against the test protocol — verifies TestScore
/// is registered as a resource kind. Defensive: catches the case
/// where someone reorders/removes the protocol's resource registration.
#[given(r#"a Naia protocol with replicated resource type "Score""#)]
fn given_protocol_with_score(_ctx: &mut TestWorldMut) {
    use naia_test_harness::{protocol, TestScore};
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestScore>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestScore must be registered as a resource"
    );
}

/// Given a Naia protocol with replicated resource type "MatchState".
#[given(r#"a Naia protocol with replicated resource type "MatchState""#)]
fn given_protocol_with_matchstate(_ctx: &mut TestWorldMut) {
    use naia_test_harness::{protocol, TestMatchState};
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestMatchState>();
    assert!(p.resource_kinds.is_resource(&kind), "TestMatchState must be registered as a resource");
}

/// Given a Naia protocol with delegable replicated resource type "PlayerSelection".
#[given(r#"a Naia protocol with delegable replicated resource type "PlayerSelection""#)]
fn given_protocol_with_player_selection(_ctx: &mut TestWorldMut) {
    use naia_test_harness::{protocol, TestPlayerSelection};
    let p = protocol();
    let kind = naia_shared::ComponentKind::of::<TestPlayerSelection>();
    assert!(
        p.resource_kinds.is_resource(&kind),
        "TestPlayerSelection must be registered as a resource"
    );
}

/// Given a server with `MatchState { phase: N }` and one connected client.
///
/// Composite Given: start server, connect a client, insert MatchState as static,
/// spin 30 ticks for replication. Used by removal/re-insert resource scenarios.
#[given(r#"a server with MatchState \{ phase: {int} \} and one connected client"#)]
fn given_server_with_matchstate_and_client(ctx: &mut TestWorldMut, phase: u8) {
    use naia_test_harness::TestMatchState;
    ensure_server_started(ctx);
    connect_client(ctx);
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_static_resource(TestMatchState::new(phase)),
                "insert MatchState should succeed for fresh type"
            );
        });
    });
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

/// Given alice holds authority on PlayerSelection.
///
/// Composite Given: alice requests authority and waits for Granted.
/// Requires the server to have PlayerSelection already inserted and alice connected.
#[given("alice holds authority on PlayerSelection")]
fn given_alice_holds_authority(ctx: &mut TestWorldMut) {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::TestPlayerSelection;
    let scenario = ctx.scenario_mut();
    let alice_key = scenario.bdd_get(&crate::steps::world_helpers::client_key_storage("alice"))
        .expect("alice not connected");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            assert!(server.configure_resource::<TestPlayerSelection>(naia_server::ReplicationConfig::delegated()));
        });
    });
    scenario.spec_expect("alice holds authority: Available before request", |ctx| {
        ctx.client(alice_key, |cl| {
            (cl.resource_authority_status::<TestPlayerSelection>() == Some(EntityAuthStatus::Available))
                .then_some(())
        })
    });
    scenario.mutate(|mctx| {
        mctx.client(alice_key, |cl| {
            assert!(cl.request_resource_authority::<TestPlayerSelection>().is_ok());
        });
    });
    scenario.spec_expect("alice holds authority: Granted", |ctx| {
        ctx.client(alice_key, |cl| {
            (cl.resource_authority_status::<TestPlayerSelection>() == Some(EntityAuthStatus::Granted))
                .then_some(())
        })
    });
    scenario.allow_flexible_next();
}

/// Given alice has set selected_id to {int}.
#[given("alice has set selected_id to {int}")]
fn given_alice_has_set_selected_id(ctx: &mut TestWorldMut, value: u16) {
    use naia_test_harness::TestPlayerSelection;
    let scenario = ctx.scenario_mut();
    let alice_key = scenario.bdd_get(&crate::steps::world_helpers::client_key_storage("alice"))
        .expect("alice not connected");
    scenario.mutate(|mctx| {
        mctx.client(alice_key, |cl| {
            cl.mutate_resource::<TestPlayerSelection, _, _>(|r| { *r.selected_id = value; });
        });
    });
    for _ in 0..10 {
        scenario.mutate(|_| {});
    }
}

/// Given a server with `PlayerSelection { selected_id: 0 }` and connected client "alice".
///
/// Composite Given: ensure server, connect alice, insert
/// PlayerSelection(0), spin 30 ticks for replication. Used by the
/// authority/delegation resource scenarios.
#[given(r#"a server with PlayerSelection \{ selected_id: 0 \} and connected client "alice""#)]
fn given_server_with_player_selection_and_alice(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestPlayerSelection;
    ensure_server_started(ctx);
    let _ = connect_test_client(ctx, "alice");
    let scenario = ctx.scenario_mut();
    scenario.mutate(|c| {
        c.server(|server| {
            assert!(
                server.insert_resource(TestPlayerSelection::new(0)),
                "insert PlayerSelection should succeed for fresh type"
            );
        });
    });
    for _ in 0..30 {
        scenario.mutate(|_| {});
    }
}

