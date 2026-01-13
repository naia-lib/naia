#![allow(unused_imports)]

use std::time::Duration;

use naia_client::{ClientConfig, JitterBufferType, ReplicationConfig as ClientReplicationConfig};
use naia_server::{ReplicationConfig, RoomKey, ServerConfig};
use naia_shared::{AuthorityError, EntityAuthStatus, Protocol, Request, Response, Tick};

use naia_test::{
    protocol, Auth, ClientConnectEvent, ClientDisconnectEvent, ClientEntityAuthDeniedEvent,
    ClientEntityAuthGrantedEvent, ClientEntityAuthResetEvent, ClientKey, ClientRejectEvent,
    ExpectCtx, Position, Scenario, ServerAuthEvent, ServerConnectEvent, ServerDisconnectEvent,
    ToTicks,
};

// Test protocol types (channels and messages)
use naia_test::test_protocol::{
    OrderedChannel, ReliableChannel, RequestResponseChannel, SequencedChannel,
    TestMessage, TestRequest, TestResponse, TickBufferedChannel, UnorderedChannel,
    UnreliableChannel,
};

mod _helpers;
use _helpers::{client_connect, server_and_client_connected, server_and_client_disconnected, test_client_config};


// ============================================================================
// Server Events Api Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/12_server_events_api.md
// ============================================================================

/// Inserts/updates/removes are one-shot and non-duplicated
/// Contract: [server-events-00], [server-events-01]
///
/// Given server spawns E, updates a component, then removes it in one tick;
/// when main loop calls `take_inserts`, `take_updates`, `take_removes` once;
/// then each change appears exactly once and subsequent calls that tick return nothing for those changes.
#[test]
fn inserts_updates_removes_are_one_shot_and_non_duplicated() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Spawn entity with Position component
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_e, local_entity) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_e);
            (entity_e, local_entity)
        })
    });

    // Wait for spawn event
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Update component
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 10.0;
                }
            }
        });
    });

    // Verify update was applied
    scenario.expect(|ctx| {
        ctx.server(|s| {
            if let Some(e) = s.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    (*pos.x - 10.0).abs() < 0.001
                } else {
                    false
                }
            } else {
                false
            }
        })
        .then_some(())
    });

    // Remove component
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.remove_component::<Position>();
            }
        });
    });

    // Verify remove was applied
    scenario.expect(|ctx| {
        (!ctx.server(|s| {
            s.entity(&entity_e)
                .map(|e| e.has_component::<Position>())
                .unwrap_or(false)
        }))
        .then_some(())
    });

    // TODO: Verify that insert/update/remove events appear exactly once
    // TODO: Verify that subsequent calls return nothing for those changes
    // This requires access to take_inserts/take_updates/take_removes from Events API
}

/// Component update events reflect correct multiplicity per user
/// Contract: [server-events-02], [server-events-03]
///
/// Given component replicated to multiple users; when server changes component once;
/// then `take_updates` returns one event per in-scope user with no duplicates or missing entries.
#[test]
fn component_update_events_reflect_correct_multiplicity_per_user() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Spawn entity with Position component
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_e, local_entity) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_e);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_e);
            (entity_e, local_entity)
        })
    });

    // Wait for both clients to see the entity
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Update component once
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 10.0;
                }
            }
        });
    });

    // TODO: Verify that take_updates returns one event per in-scope user (A and B)
    // TODO: Verify no duplicates or missing entries
}

/// Message events grouped correctly by channel and type
/// Contract: [server-events-04]
///
/// Given multiple message types from multiple users across multiple channels in one tick;
/// when Events API drains messages; then grouping matches documented structure (by channel/type/user),
/// each message appears once, and second call in same tick yields none.
#[test]
fn message_events_grouped_correctly_by_channel_and_type() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Send multiple messages from multiple users on multiple channels
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // A sends on ReliableChannel
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            // B sends on ReliableChannel
            server.send_message::<ReliableChannel, _>(&client_b_key, &TestMessage::new(2));
            // A sends on OrderedChannel
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(10));
        });
    });

    // Verify messages are grouped correctly
    scenario.expect(|ctx| {
        let a_reliable: Vec<u32> = ctx.client(client_a_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });
        let b_reliable: Vec<u32> = ctx.client(client_b_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });
        let a_ordered: Vec<u32> = ctx.client(client_a_key, |c| {
            c.read_message::<OrderedChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });

        // A should receive 1 on ReliableChannel
        let a_has_1 = a_reliable.contains(&1);
        // B should receive 2 on ReliableChannel
        let b_has_2 = b_reliable.contains(&2);
        // A should receive 10 on OrderedChannel
        let a_has_10 = a_ordered.contains(&10);
        // No cross-contamination
        let a_no_2 = !a_reliable.contains(&2);
        let b_no_1 = !b_reliable.contains(&1);
        let b_no_ordered = !ctx.client(client_b_key, |c| {
            c.read_message::<OrderedChannel, TestMessage>()
                .next()
                .is_some()
        });

        (a_has_1 && b_has_2 && a_has_10 && a_no_2 && b_no_1 && b_no_ordered).then_some(())
    });

    // TODO: Verify second call in same tick yields none (requires access to Events API directly)
}

/// Request/response events via Events API are drained and do not reappear
/// Contract: [server-events-05], [server-events-06]
///
/// Given multiple client requests and server responses in a tick;
/// when Events API drains request/response events; then each appears exactly once
/// and does not reappear later that tick, with no silent loss.
#[test]
fn request_response_events_via_events_api_are_drained_and_do_not_reappear() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Both clients send requests
    let (response_key_a, response_key_b) = scenario.mutate(|ctx| {
        let key_a = ctx.client(client_a_key, |c| {
            c.send_request::<ReliableChannel, naia_test::test_protocol::TestRequest>(
                &naia_test::test_protocol::TestRequest::new("query_a"),
            )
            .expect("Failed to send request")
        });
        let key_b = ctx.client(client_b_key, |c| {
            c.send_request::<ReliableChannel, naia_test::test_protocol::TestRequest>(
                &naia_test::test_protocol::TestRequest::new("query_b"),
            )
            .expect("Failed to send request")
        });
        (key_a, key_b)
    });

    // Server receives and responds to both requests
    let response_ids = scenario.expect(|ctx| {
        ctx.server(|server| {
            let mut ids = Vec::new();
            for (client_key, response_id, _request) in
                server.read_request::<ReliableChannel, naia_test::test_protocol::TestRequest>()
            {
                if client_key == client_a_key || client_key == client_b_key {
                    ids.push((client_key, response_id));
                }
            }
            if ids.len() == 2 {
                Some(ids)
            } else {
                None
            }
        })
    });

    // Server sends responses
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            for (client_key, response_id) in &response_ids {
                let response_send_key = naia_shared::ResponseSendKey::new(*response_id);
                if *client_key == client_a_key {
                    server.send_response(
                        &response_send_key,
                        &naia_test::test_protocol::TestResponse::new("result_a"),
                    );
                } else if *client_key == client_b_key {
                    server.send_response(
                        &response_send_key,
                        &naia_test::test_protocol::TestResponse::new("result_b"),
                    );
                }
            }
        });
    });

    // Wait for both clients to have their responses
    scenario.expect(|ctx| {
        let a_has = ctx.client(client_a_key, |c| c.has_response(&response_key_a));
        let b_has = ctx.client(client_b_key, |c| c.has_response(&response_key_b));
        (a_has && b_has).then_some(())
    });

    // Verify clients receive responses
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            if let Some(response) = c.receive_response(&response_key_a) {
                assert_eq!(response.result, "result_a");
            } else {
                panic!("Expected response for client A");
            }
        });
        ctx.client(client_b_key, |c| {
            if let Some(response) = c.receive_response(&response_key_b) {
                assert_eq!(response.result, "result_b");
            } else {
                panic!("Expected response for client B");
            }
        });
    });

    // TODO: Verify that request/response events appear exactly once
    // TODO: Verify they don't reappear on subsequent calls
}

/// Accessing non-existent entity yields safe failure, not panic
/// Contract: [server-events-07], [server-events-08]
///
/// Given no entity with a certain ID; when code attempts to access it via read/write APIs;
/// then APIs return "not found"/`None`/error without panicking or corrupting state.
#[test]
fn accessing_non_existent_entity_yields_safe_failure_not_panic() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Create a fake entity key that doesn't correspond to any real entity
    // We'll allocate a key from a temporary scenario that we know doesn't exist in the main scenario
    let fake_entity = {
        let temp_protocol = protocol();
        let mut temp_scenario = Scenario::new();
        temp_scenario.server_start(ServerConfig::default(), temp_protocol);
        // Spawn an entity to get a key, then we'll use this key which won't exist in main scenario
        let (fake_key, _) = temp_scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.spawn(|mut e| {
                    e.insert_component(Position::new(0.0, 0.0));
                })
            })
        });
        fake_key
    };

    // Verify accessing non-existent entity returns None/error safely
    scenario.mutate(|ctx| {
        // Server side - should return None
        let server_entity = ctx.server(|server| server.entity(&fake_entity).is_none());

        // Client side - should return None
        let client_entity = ctx.client(client_a_key, |c| c.entity(&fake_entity).is_none());

        assert!(server_entity);
        assert!(client_entity);
    });
}

/// Accessing an entity after despawn is safely rejected
/// Contract: [server-events-09], [server-events-10]
///
/// Given E was spawned then despawned; when code attempts to read/mutate E after despawn;
/// then calls fail gracefully and do not recreate E or panic.
#[test]
fn accessing_an_entity_after_despawn_is_safely_rejected() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Spawn entity
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_e, local_entity) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_e);
            (entity_e, local_entity)
        })
    });

    // Wait for entity to be visible
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Despawn entity
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.despawn(&entity_e);
        });
    });

    // Wait for entity to be removed from both server and client
    scenario.expect(|ctx| {
        let server_removed = !ctx.server(|s| s.has_entity(&entity_e));
        let client_removed = ctx.client(client_a_key, |c| c.entity(&entity_e).is_none());
        (server_removed && client_removed).then_some(())
    });

    // Verify accessing despawned entity returns None/error safely
    scenario.mutate(|ctx| {
        // Server side - should return None
        let server_entity = ctx.server(|server| server.entity(&entity_e).is_none());

        // Client side - should return None
        let client_entity = ctx.client(client_a_key, |c| c.entity(&entity_e).is_none());

        assert!(server_entity);
        assert!(client_entity);
    });
}

/// Mutating out-of-scope entity for a given user is ignored or errors predictably
/// Contract: [server-events-11], [server-events-12]
///
/// Given E not in A's scope; when A tries to mutate E via client APIs or server applies per-user operation assuming A sees E;
/// then Naia either ignores the operation or returns a defined error, without corrupting scoped state.
#[test]
fn mutating_out_of_scope_entity_for_a_given_user_is_ignored_or_errors_predictably() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Spawn entity but don't include it in A's scope
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            })
        })
    });

    // Verify A cannot see the entity
    scenario.expect(|ctx| (!ctx.client(client_a_key, |c| c.has_entity(&entity_e))).then_some(()));

    // Verify that A cannot mutate the entity via client APIs
    // entity_mut() should return None for out-of-scope entities
    let can_mutate =
        scenario.mutate(|ctx| ctx.client(client_a_key, |c| c.entity_mut(&entity_e).is_some()));
    assert!(
        !can_mutate,
        "entity_mut() should return None for out-of-scope entities, preventing mutation"
    );
}

/// Sending messages or requests on a disconnected or rejected connection is safe
/// Contract: [server-events-13]
///
/// Given a connection that is disconnected or rejected; when code sends a message/request on it;
/// then attempt is ignored or returns clear error, and does not resurrect connection or panic.
#[test]
fn sending_messages_or_requests_on_a_disconnected_or_rejected_connection_is_safe() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| ctx.server(|server| server.make_room().key()));

    let client_a_key = client_connect(
        &mut scenario,
        &room_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Disconnect
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            c.disconnect();
        });
    });

    // Wait for disconnect
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_a_key))).then_some(()));

    // Try to send message after disconnect
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            // This should either be ignored or return an error, not panic
            c.send_message::<ReliableChannel, _>(&TestMessage::new(42));
        });
    });

    // TODO: Verify message is ignored or error is returned
    // TODO: Verify connection is not resurrected
}
