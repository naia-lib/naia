use naia_client::ReplicationConfig;
use naia_test::{
    harness::Scenario,
    protocol, Auth, Position,
};

/// Test: single client spawn replicates to server
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new(protocol());

    scenario.server_start();
    let client_a_key = scenario.client_connect("Client A", Auth::new("client_a", "password"));
    
    // Mutate phase: client spawns entity
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut spawned_entity| {
                spawned_entity
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });
    
    // Expect phase: server has entity
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server.has_entity(entity_a)
        })
    });
}

/// Test: two clients see the same logical entity
#[test]
fn harness_two_clients_entity_mapping() {
    let mut scenario = Scenario::new(protocol());
    scenario.server_start();

    let client_a_key = scenario.client_connect("Client A", Auth::new("client_a", "password"));
    let client_b_key = scenario.client_connect("Client B", Auth::new("client_b", "password"));
    
    let room_key = *scenario.main_room_key().unwrap();

    // Mutate phase: client A spawns entity A
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut spawned_entity| {
                spawned_entity.configure_replication(ReplicationConfig::Public).insert_component(Position::new(10.0, 20.0));
            })
        })
    });

    // Wait for entity to replicate to server
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server.has_entity(entity_a)
        })
    });

    // Now include B in scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.entity_mut(entity_a).unwrap().enter_room(&room_key);
            server.include_in_scope(client_b_key, entity_a);
        });
    });

    // Expect phase: client B sees entity
    scenario.expect(|ctx| {
        ctx.client(client_b_key, |client_b| {
            client_b.sees(entity_a)
        })
    });

    // Additional expect: both clients report same position after A changes it
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_a_mut) = client_a.entity_mut(entity_a) {
                entity_a_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    scenario.expect(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.entity(entity_a).position_is(100.0, 200.0)
        }) && ctx.client(client_b_key, |client_b| {
            client_b.entity(entity_a).position_is(100.0, 200.0)
        })
    });
}