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

/// Given a server with `PlayerSelection { selected_id: 0 }` and connected client "alice".
///
/// Composite Given: ensure server, connect alice, insert
/// PlayerSelection(0), spin 30 ticks for replication. Used by the
/// authority/delegation resource scenarios.
#[given(r#"a server with PlayerSelection \{ selected_id: 0 \} and connected client "alice""#)]
fn given_server_with_player_selection_and_alice(ctx: &mut TestWorldMut) {
    use naia_test_harness::TestPlayerSelection;
    use crate::steps::world_helpers::{connect_test_client, ensure_server_started};
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

