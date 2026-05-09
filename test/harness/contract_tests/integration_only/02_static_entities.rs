#![allow(unused_imports)]

use naia_server::{ReplicationConfig, ServerConfig};
use naia_shared::Protocol;

use naia_test_harness::{protocol, Auth, Position, Scenario};

mod _helpers;

/// Static entity panics on insert_component after construction.
///
/// Contract: [static-entity-05]
/// Calling `insert_component` on a static entity after the `spawn()` closure
/// has returned must panic.  This is a protocol invariant enforced at the
/// server-entity handle level; it cannot be tested via BDD (panics are
/// unergonomic in Gherkin).
#[test]
#[should_panic]
fn static_entity_panics_on_insert_after_construction() {
    let mut scenario = Scenario::new();
    let proto = protocol();
    scenario.server_start(ServerConfig::default(), proto);

    // Spawn a static entity — `as_static()` during construction is fine.
    let entity_key = scenario.mutate(|mctx| {
        let (key, _) = mctx.server(|server| {
            server.spawn(|mut entity| {
                entity.as_static().insert_component(Position::new(0.0, 0.0));
            })
        });
        key
    });

    // Attempting to insert another component on an already-constructed static
    // entity must panic.
    scenario.mutate(|mctx| {
        mctx.server(|server| {
            if let Some(mut entity) = server.entity_mut(&entity_key) {
                // This should panic: static entity does not allow post-construction inserts.
                entity.insert_component(Position::new(1.0, 1.0));
            }
        });
    });
}
