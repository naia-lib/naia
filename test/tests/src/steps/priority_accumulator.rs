//! Step bindings for Priority Accumulator contract (20_priority_accumulator.spec.md)
//!
//! Covers the three BDD obligations that require a real server+client
//! round-trip:
//!   - AB-BDD-1: spawn-burst drainage under budget
//!   - B-BDD-6:  global gain override persists across send cycle
//!   - B-BDD-8:  per-entity value convergence under cross-entity reorder

use naia_test_harness::{EntityKey, Position};
use namako_engine::codegen::AssertOutcome;
use namako_engine::{given, then, when};

use crate::{TestWorldMut, TestWorldRef};

const LAST_ENTITY_KEY: &str = "last_entity";
const SPAWN_BURST_KEYS: &str = "priority_acc_burst_keys";
const ENTITY_A_KEY: &str = "priority_acc_entity_a";
const ENTITY_B_KEY: &str = "priority_acc_entity_b";

// ============================================================================
// AB-BDD-1: Spawn-burst drainage under budget
// ============================================================================

#[when("the server spawns {int} entities in one tick and scopes them for the client")]
fn when_server_spawns_n_entities_one_tick(ctx: &mut TestWorldMut, count: usize) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();

    let keys: Vec<EntityKey> = scenario.mutate(|mctx| {
        let mut keys = Vec::with_capacity(count);
        mctx.server(|server| {
            for i in 0..count {
                let (ek, _) = server.spawn(|mut entity| {
                    entity.insert_component(Position::new(i as f32, i as f32));
                });
                keys.push(ek);
            }
            // Place all entities in the room and in-scope for the client.
            for ek in &keys {
                if let Some(mut e) = server.entity_mut(ek) {
                    e.enter_room(&room_key);
                }
            }
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                for ek in &keys {
                    scope.include(ek);
                }
            }
        });
        keys
    });

    scenario.bdd_store(SPAWN_BURST_KEYS, keys);
}

#[then("the client eventually observes all {int} spawned entities")]
fn then_client_eventually_observes_all_spawned(
    ctx: &TestWorldRef,
    expected: usize,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let keys: Vec<EntityKey> = ctx
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

// ============================================================================
// B-BDD-6: Gain override persists across send cycle
// ============================================================================

#[when("the server sets the global priority gain on the last entity to {float}")]
fn when_server_sets_global_gain_on_last_entity(ctx: &mut TestWorldMut, gain: f32) {
    let scenario = ctx.scenario_mut();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            server.set_global_entity_gain(&entity_key, gain);
        });
    });
}

#[then("the global priority gain on the last entity is {float}")]
fn then_global_gain_on_last_entity_is(ctx: &TestWorldRef, expected: f32) -> AssertOutcome<()> {
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.server(|server| match server.global_entity_gain(&entity_key) {
        Some(g) if (g - expected).abs() < f32::EPSILON => AssertOutcome::Passed(()),
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

#[then("the client eventually sees the last entity")]
fn then_client_eventually_sees_last_entity(ctx: &TestWorldRef) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key: EntityKey = ctx
        .scenario()
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");

    ctx.client(client_key, |client| {
        if client.has_entity(&entity_key) {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}

#[then("the global priority gain on the last entity is still {float}")]
fn then_global_gain_on_last_entity_is_still(
    ctx: &TestWorldRef,
    expected: f32,
) -> AssertOutcome<()> {
    then_global_gain_on_last_entity_is(ctx, expected)
}

// ============================================================================
// B-BDD-8: Per-entity value convergence under cross-entity reorder
// ============================================================================

#[given("two server-owned entities A and B exist each with a replicated component in-scope for the client")]
fn given_two_entities_a_b_in_scope(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();
    let room_key = scenario.last_room();

    let (entity_a, entity_b) = scenario.mutate(|mctx| {
        let (a, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        let (b, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.insert_component(Position::new(0.0, 0.0));
            })
        });
        mctx.server(|server| {
            for ek in [&a, &b] {
                if let Some(mut e) = server.entity_mut(ek) {
                    e.enter_room(&room_key);
                }
            }
            if let Some(mut scope) = server.user_scope_mut(&client_key) {
                scope.include(&a);
                scope.include(&b);
            }
        });
        (a, b)
    });

    scenario.bdd_store(ENTITY_A_KEY, entity_a);
    scenario.bdd_store(ENTITY_B_KEY, entity_b);

    // Wait for both entities to be observable on the client before moving on.
    scenario.expect(|ectx| {
        ectx.client(client_key, |client| {
            if client.has_entity(&entity_a) && client.has_entity(&entity_b) {
                Some(())
            } else {
                None
            }
        })
    });
}

fn entity_key_for_name(ctx: &mut TestWorldMut, name: &str) -> EntityKey {
    let key_name = match name {
        "A" => ENTITY_A_KEY,
        "B" => ENTITY_B_KEY,
        other => panic!("unknown entity label '{}' in priority_accumulator step", other),
    };
    ctx.scenario_mut()
        .bdd_get::<EntityKey>(key_name)
        .unwrap_or_else(|| panic!("entity '{}' not stored", name))
}

fn entity_key_for_name_ref(ctx: &TestWorldRef, name: &str) -> EntityKey {
    let key_name = match name {
        "A" => ENTITY_A_KEY,
        "B" => ENTITY_B_KEY,
        other => panic!("unknown entity label '{}' in priority_accumulator step", other),
    };
    ctx.scenario()
        .bdd_get::<EntityKey>(key_name)
        .unwrap_or_else(|| panic!("entity '{}' not stored", name))
}

#[when("the server mutates entity {word}'s component to x={int} y={int}")]
fn when_server_mutates_entity_component(
    ctx: &mut TestWorldMut,
    name: String,
    x: i32,
    y: i32,
) {
    let entity_key = entity_key_for_name(ctx, &name);
    let scenario = ctx.scenario_mut();

    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                if let Some(mut pos) = entity.component::<Position>() {
                    *pos.x = x as f32;
                    *pos.y = y as f32;
                }
            }
        });
    });
}

#[then("the client eventually observes entity {word} at x={int} y={int}")]
fn then_client_eventually_observes_entity_at(
    ctx: &TestWorldRef,
    name: String,
    x: i32,
    y: i32,
) -> AssertOutcome<()> {
    let client_key = ctx.last_client();
    let entity_key = entity_key_for_name_ref(ctx, &name);
    let (ex, ey) = (x as f32, y as f32);

    ctx.client(client_key, |client| {
        let Some(entity) = client.entity(&entity_key) else {
            return AssertOutcome::Pending;
        };
        let Some(pos) = entity.component::<Position>() else {
            return AssertOutcome::Pending;
        };
        let cx = *pos.x;
        let cy = *pos.y;
        if (cx - ex).abs() < f32::EPSILON && (cy - ey).abs() < f32::EPSILON {
            AssertOutcome::Passed(())
        } else {
            AssertOutcome::Pending
        }
    })
}
