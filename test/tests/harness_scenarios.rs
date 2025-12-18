use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig};
use naia_server::{RoomKey, ServerConfig};
use naia_test::{
    Scenario, ClientKey,
    protocol, Auth, Position,
    AuthEvent, ConnectEvent,
};

mod test_helpers;
use test_helpers::{make_room, client_connect};

/// Test: single client spawn replicates to server
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol);
    
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
            server.has_entity(&entity_a).then_some(())
        })
    });
}

/// Test: two clients see the same logical entity
#[test]
fn harness_two_clients_entity_mapping() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"), test_protocol.clone());
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B", Auth::new("client_b", "password"), test_protocol);

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
            server.has_entity(&entity_a).then_some(())
        })
    });

    // Now include B in scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Ensure entity is in room
            server.entity_mut(&entity_a).unwrap().enter_room(&room_key);
            
            // Include entity in Client B's scope
            server.user_scope_mut(&client_b_key).unwrap().include(&entity_a);
        });
    });

    // Expect phase: client B sees entity
    scenario.expect(|ctx| {
        ctx.client(client_b_key, |client_b| {
            client_b.has_entity(&entity_a).then_some(())
        })
    });

    // Additional expect: both clients report same position after A changes it
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            if let Some(mut entity_a_mut) = client_a.entity_mut(&entity_a) {
                entity_a_mut.insert_component(Position::new(100.0, 200.0));
            }
        });
    });

    scenario.expect(|ctx| {
        let client_a_ok = ctx.client(client_a_key, |client_a| {
            if let Some(entity_ref) = client_a.entity(&entity_a) {
                if let Some(pos) = entity_ref.component::<Position>() {
                    (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        });
        let client_b_ok = ctx.client(client_b_key, |client_b| {
            if let Some(entity_ref) = client_b.entity(&entity_a) {
                if let Some(pos) = entity_ref.component::<Position>() {
                    (*pos.x - 100.0).abs() < 0.001 && (*pos.y - 200.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        });
        (client_a_ok && client_b_ok).then_some(())
    });
}
