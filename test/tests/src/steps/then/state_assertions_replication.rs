//! Then-step bindings: entity replication, priority, scope, and resource assertions.

use crate::steps::prelude::*;
use crate::steps::world_helpers::{last_entity_ref, named_client_ref};

// ──────────────────────────────────────────────────────────────────────
// Entity replication assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the entity spawns on the client with the replicated component.
#[then("the entity spawns on the client with the replicated component")]
fn then_entity_spawns_on_client_with_replicated_component(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if let Some(entity) = client.entity(&entity_key) {
            if entity.has_component::<Position>() {
                AssertOutcome::Passed(())
            } else {
                AssertOutcome::Pending
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client observes the component update.
#[then("the client observes the component update")]
fn then_client_observes_component_update(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use crate::steps::world_helpers_connect::assert_client_position_eq;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    let expected: (f32, f32) = ctx.scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No component value stored");
    assert_client_position_eq(ctx, client_key, entity_key, expected)
}

/// Then the client observes the server value.
///
/// Used after `Given the client modifies the component locally` —
/// asserts that the server-authoritative value overrides the
/// client-local modification.
#[then("the client observes the server value")]
fn then_client_observes_server_value(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use crate::steps::world_helpers_connect::assert_client_position_eq;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    let server_value: (f32, f32) = ctx.scenario()
        .bdd_get(LAST_COMPONENT_VALUE_KEY)
        .expect("No server component value stored");
    assert_client_position_eq(ctx, client_key, entity_key, server_value)
}

/// Then the entity GlobalEntity remains unchanged.
///
/// EntityKey is the harness abstraction over Naia's GlobalEntity.
/// Stable identity throughout an entity's lifetime is the contract.
#[then("the entity GlobalEntity remains unchanged")]
fn then_entity_global_entity_remains_unchanged(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let initial: naia_test_harness::EntityKey = ctx
        .scenario()
        .bdd_get(INITIAL_ENTITY_KEY)
        .expect("No initial entity key stored");
    let current = last_entity_ref(ctx);
    if initial == current {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Failed(format!(
            "GlobalEntity changed: initial={:?}, current={:?}",
            initial, current
        ))
    }
}

// ──────────────────────────────────────────────────────────────────────
// Priority accumulator assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the client eventually observes all N spawned entities.
#[then("the client eventually observes all {int} spawned entities")]
fn then_client_eventually_observes_all_spawned(
    ctx: &TestWorldRef,
    expected: usize,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let keys: Vec<naia_test_harness::EntityKey> = ctx
        .scenario()
        .bdd_get(SPAWN_BURST_KEYS)
        .expect("spawn-burst keys missing");
    if keys.len() != expected {
        return AssertOutcome::Failed(format!(
            "stored {} burst keys but scenario expected {}",
            keys.len(),
            expected
        ));
    }
    ctx.client(client_key, |client| {
        if keys.iter().all(|k| client.has_entity(k)) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the global priority gain on the last entity is {float}.
#[then("the global priority gain on the last entity is {float}")]
fn then_global_gain_on_last_entity_is(
    ctx: &TestWorldRef,
    expected: f32,
) -> AssertOutcome<()> {
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| match server.global_entity_gain(&entity_key) {
        Some(g) if (g - expected).abs() < f32::EPSILON => {
            AssertOutcome::Passed(())
        }
        Some(g) => AssertOutcome::Failed(format!(
            "global gain is {} but expected {}",
            g, expected
        )),
        None => AssertOutcome::Failed(format!(
            "no gain override is set (expected {})",
            expected
        )),
    })
}

/// Then the client eventually sees the last entity.
#[then("the client eventually sees the last entity")]
fn then_client_eventually_sees_last_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the global priority gain on the last entity is still {float}.
///
/// Same predicate as `is {float}`, distinct phrase to read naturally
/// after a follow-up tick step.
#[then("the global priority gain on the last entity is still {float}")]
fn then_global_gain_on_last_entity_is_still(
    ctx: &TestWorldRef,
    expected: f32,
) -> AssertOutcome<()> {
    then_global_gain_on_last_entity_is(ctx, expected)
}

/// Then the client eventually observes entity {label} at x={int} y={int}.
///
/// `label` is "A" or "B"; resolves via `entity_label_to_key_storage`.
#[then("the client eventually observes entity {word} at x={int} y={int}")]
fn then_client_eventually_observes_entity_at(
    ctx: &TestWorldRef,
    label: String,
    x: i32,
    y: i32,
) -> AssertOutcome<()> {
    use naia_test_harness::Position;
    let client_key = ctx.last_client();
    let entity_key: naia_test_harness::EntityKey = ctx
        .scenario()
        .bdd_get(entity_label_to_key_storage(&label))
        .unwrap_or_else(|| panic!("entity '{}' not stored", label));
    let (ex, ey) = (x as f32, y as f32);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        if (*pos.x - ex).abs() < f32::EPSILON && (*pos.y - ey).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Scope-exit (Persist) assertions
// ──────────────────────────────────────────────────────────────────────

/// Then the client still has the entity.
///
/// Confirms ScopeExit::Persist prevented the Despawn when the entity
/// went out-of-scope.
#[then("the client still has the entity")]
fn then_client_still_has_entity(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Failed(
                "Entity was despawned on client despite ScopeExit::Persist".into(),
            )
        }
    })
}

/// Then the client entity position is still 0.0.
///
/// Confirms no update leaked through while the entity was Paused.
#[then("the client entity position is still 0.0")]
fn then_client_entity_position_still_zero(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::Position;
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Failed("Entity absent despite ScopeExit::Persist".into());
        };
        let Some(pos) = entity.component::<Position>() else { return AssertOutcome::Pending; };
        if (*pos.x).abs() < f32::EPSILON { AssertOutcome::Passed(()) }
        else { AssertOutcome::Failed(format!("Position leaked while out-of-scope: x={}", *pos.x)) }
    })
}

/// Then the client entity position becomes 100.0.
///
/// Polling — confirms accumulated updates from the Paused period
/// arrive after re-entry.
#[then("the client entity position becomes 100.0")]
fn then_client_entity_position_becomes_hundred(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{Position};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        if (*pos.x - 100.0).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client entity has ImmutableLabel.
#[then("the client entity has ImmutableLabel")]
fn then_client_entity_has_label(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        if entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

/// Then the client entity does not have ImmutableLabel.
#[then("the client entity does not have ImmutableLabel")]
fn then_client_entity_no_label(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    use naia_test_harness::{ImmutableLabel};
    let client_key = ctx.last_client();
    let entity_key = last_entity_ref(ctx);
    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        if !entity.has_component::<ImmutableLabel>() {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Entity publication — scope-membership for named clients
// ──────────────────────────────────────────────────────────────────────

/// Internal helper: server-side scope-membership check for a labeled
/// client. Used by all four "the entity is{,n't,becomes} in/out-of-scope
/// for client X" assertions below.
fn check_entity_in_scope(ctx: &TestWorldRef, label: &str) -> bool {
    let client_key = named_client_ref(ctx, label);
    let entity_key = last_entity_ref(ctx);
    ctx.server(|server| {
        if let Some(scope) = server.user_scope(&client_key) {
            scope.has(&entity_key)
        } else {
            false
        }
    })
}

/// Then the entity is in-scope for client A.
#[then("the entity is in-scope for client A")]
fn then_entity_in_scope_for_client_a(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "A"),
        "Expected entity to be in-scope for client A, but it was not"
    );
}

/// Then the entity is in-scope for client B.
#[then("the entity is in-scope for client B")]
fn then_entity_in_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        check_entity_in_scope(ctx, "B"),
        "Expected entity to be in-scope for client B, but it was not"
    );
}

/// Then the entity is out-of-scope for client B.
#[then("the entity is out-of-scope for client B")]
fn then_entity_out_of_scope_for_client_b(ctx: &TestWorldRef) {
    assert!(
        !check_entity_in_scope(ctx, "B"),
        "Expected entity to be out-of-scope for client B, but it was in-scope"
    );
}

/// Then the entity becomes out-of-scope for client B.
///
/// Polling variant of the above — used after an unpublish where the
/// scope removal propagates asynchronously.
#[then("the entity becomes out-of-scope for client B")]
fn then_entity_becomes_out_of_scope_for_client_b(
    ctx: &TestWorldRef,
) -> AssertOutcome<()> {
    if !check_entity_in_scope(ctx, "B") {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then client {label} observes replication config as {config} for the entity.
///
/// Polls until the named client's entity reports the expected
/// `ReplicationConfig`. Covers [entity-publication-observability].
#[then("client {word} observes replication config as {word} for the entity")]
fn then_client_observes_replication_config(
    ctx: &TestWorldRef,
    label: String,
    config_name: String,
) -> AssertOutcome<()> {
    use naia_client::ReplicationConfig as ClientReplicationConfig;
    let client_key = named_client_ref(ctx, &label);
    let entity_key = last_entity_ref(ctx);
    let expected = match config_name.as_str() {
        "Public" => ClientReplicationConfig::Public,
        "Private" => ClientReplicationConfig::Private,
        "Delegated" => ClientReplicationConfig::Delegated,
        other => {
            return AssertOutcome::Failed(format!(
                "Unknown replication config: '{}'",
                other
            ))
        }
    };
    ctx.client(client_key, |c| {
        if let Some(entity) = c.entity(&entity_key) {
            match entity.replication_config() {
                Some(config) if config == expected => AssertOutcome::Passed(()),
                Some(other) => AssertOutcome::Failed(format!(
                    "Expected replication_config {:?}, got {:?}",
                    expected, other
                )),
                None => AssertOutcome::Pending,
            }
        } else {
            AssertOutcome::Pending
        }
    })
}

// ──────────────────────────────────────────────────────────────────────
// Replicated resources — client-side observability
// ──────────────────────────────────────────────────────────────────────

/// Then the client's Score is present.
#[then("the client's Score is present")]
fn then_client_has_score(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    if ctx.client(key, |c| c.has_resource::<TestScore>()) {
        AssertOutcome::Passed(())
    } else {
        AssertOutcome::Pending
    }
}

/// Then the client's Score.home equals 0.
#[then("the client's Score.home equals 0")]
fn then_client_score_home_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.home)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

/// Then the client's Score.away equals 0.
#[then("the client's Score.away equals 0")]
fn then_client_score_away_0(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_test_harness::TestScore;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource::<TestScore, _, _>(|s| *s.away)) {
        Some(0) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

/// Then alice's authority status for PlayerSelection is "Granted".
#[then(r#"alice's authority status for PlayerSelection is "Granted""#)]
fn then_alice_auth_granted(ctx: &TestWorldRef) -> AssertOutcome<()> {
    use naia_shared::EntityAuthStatus;
    use naia_test_harness::TestPlayerSelection;
    let key = ctx.last_client();
    match ctx.client(key, |c| c.resource_authority_status::<TestPlayerSelection>()) {
        Some(EntityAuthStatus::Granted) => AssertOutcome::Passed(()),
        _ => AssertOutcome::Pending,
    }
}

