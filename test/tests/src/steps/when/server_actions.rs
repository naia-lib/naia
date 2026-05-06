//! When-step bindings: server-initiated state changes.

use naia_test_harness::EntityKey;
use namako_engine::when;

use crate::steps::world_helpers::LAST_ENTITY_KEY;
use crate::TestWorldMut;

/// When the server adds the entity to the client's room.
///
/// Used by scope-propagation tests where the entity arrives in the
/// client's already-occupied room mid-scenario.
#[when("the server adds the entity to the client's room")]
fn when_server_adds_entity_to_room(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let room_key = scenario.last_room();
    let entity_key: EntityKey = scenario
        .bdd_get(LAST_ENTITY_KEY)
        .expect("No entity has been created");
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                entity.enter_room(&room_key);
            }
        });
    });
}

/// When the server performs an idle tick.
///
/// Advances one server tick with no mutations. Used by update-
/// candidate-set tests to confirm idle ticks produce no dirty work.
#[when("the server performs an idle tick")]
fn when_server_performs_idle_tick(ctx: &mut TestWorldMut) {
    ctx.scenario_mut().mutate(|_| {});
}
