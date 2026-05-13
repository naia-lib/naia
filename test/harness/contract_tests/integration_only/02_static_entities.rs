#![allow(unused_imports)]

use naia_client::Publicity;
use naia_server::{ReplicationConfig, ServerConfig};
use naia_shared::Protocol;

use naia_test_harness::{protocol, Auth, Position, Scenario};

mod _helpers;

/// Server-owned static entity panics on insert_component after construction.
///
/// Contract: [static-entity-05]
/// Calling `insert_component` on a static entity after the `spawn()` closure
/// has returned must panic.  This is a protocol invariant enforced at the
/// server-entity handle level; it cannot be tested via BDD (panics are
/// unergonomic in Gherkin).
#[test]
#[should_panic]
fn server_static_entity_panics_on_insert_after_construction() {
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

/// Client-owned static entity panics on insert_component after construction.
///
/// Contract: [client-static-03]
/// Calling `insert_component` on a client-owned static entity after the
/// `spawn_static()` closure has returned must panic.  Mirrors the server-side
/// invariant above for client-authoritative entities.
///
/// No server connection is required: the panic guard is enforced locally on
/// the `EntityMut` handle, independent of network state.
#[test]
#[should_panic]
fn client_static_entity_panics_on_insert_after_construction() {
    use naia_client::ClientConfig;
    use naia_test_harness::Auth;

    let mut scenario = Scenario::new();
    let proto = protocol();
    scenario.server_start(ServerConfig::default(), proto.clone());

    let client_key = scenario.client_start(
        "client",
        Auth::new("user", "password"),
        ClientConfig::default(),
        proto,
    );

    // Spawn a static client entity — components inserted inside closure are fine.
    let entity_key = scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            client.spawn_static(|mut entity| {
                entity
                    .configure_replication(Publicity::Public)
                    .insert_component(Position::new(0.0, 0.0));
            })
        })
    });

    // Attempting to insert another component after construction must panic.
    scenario.mutate(|mctx| {
        mctx.client(client_key, |client| {
            if let Some(mut entity) = client.entity_mut(&entity_key) {
                // This should panic: static entity does not allow post-construction inserts.
                entity.insert_component(Position::new(1.0, 1.0));
            }
        });
    });
}
