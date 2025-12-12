use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig};
use naia_server::{AuthEvent, ConnectEvent as ServerConnectEvent, RoomKey, ServerConfig};
use naia_test::{
    Scenario, ClientKey,
    protocol, Auth, Position,
};

/// Test: single client spawn replicates to server
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new(protocol());

    scenario.server_start(ServerConfig::default());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"));
    
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
    let mut scenario = Scenario::new(protocol());

    scenario.server_start(ServerConfig::default());

    let room_key = make_room(&mut scenario);

    let client_a_key = client_connect(&mut scenario, &room_key, "Client A", Auth::new("client_a", "password"));
    let client_b_key = client_connect(&mut scenario, &room_key, "Client B", Auth::new("client_b", "password"));

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

fn make_room(scenario: &mut Scenario) -> RoomKey {
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.make_room().key()
        })
    })
}

fn client_connect(scenario: &mut Scenario, room_key: &RoomKey, client_name: &str, client_auth: Auth) -> ClientKey {
    // Create client config for tests (fast handshake, no jitter buffer)
    let mut client_config = ClientConfig::default();
    client_config.send_handshake_interval = Duration::from_millis(0);
    client_config.jitter_buffer = JitterBufferType::Bypass;
    
    let client_key = scenario.client_start(client_name, client_auth.clone(), client_config);

    // Client: read auth event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some((incoming_client_key, incoming_auth)) = server.read_event::<AuthEvent<Auth>>().next() {
                if incoming_client_key == client_key && incoming_auth == client_auth {
                    return Some(incoming_client_key);
                }
            }
            return None;
        })
    });

    // Server: accept connection
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.accept_connection(&client_key);
        });
    });

    // Server: read connect event
    scenario.expect(|ctx| {
        ctx.server(|server| {
            if let Some(client_key) = server.read_event::<ServerConnectEvent>() {
                if client_key == client_key {
                    return Some(());
                }
            }
            return None;
        })
    });

    // Server: add client to room
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.room_mut(&room_key).expect("room to exist").add_user(&client_key);
        });
    });

    client_key
}