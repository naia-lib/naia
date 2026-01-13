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
// Client Events Api Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/13_client_events_api.md
// ============================================================================

/// Client spawn/insert/update/remove events occur once per change and drain cleanly
/// Contract: [client-events-00], [client-events-01], [client-events-02]
///
/// Given E is spawned, component inserted, updated, then removed while in A's scope;
/// when A processes events for those ticks; then A sees one spawn, one insert, appropriate updates, and one remove,
/// and already-drained events do not reappear.
#[test]
fn client_spawn_insert_update_remove_events_occur_once_per_change_and_drain_cleanly() {
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

    scenario.expect(|_ctx| Some(()));

    // Remove component
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                e.remove_component::<Position>();
            }
        });
    });

    // TODO: Verify client sees one spawn, one insert, appropriate updates, and one remove
    // TODO: Verify already-drained events do not reappear
}

/// Client never sees update or remove events for entities that were never in scope
/// Contract: [client-events-03], [client-events-04]
///
/// Given entities created/destroyed entirely while A is out of scope;
/// when A drains events; then A sees no events for those entities.
#[test]
fn client_never_sees_update_or_remove_events_for_entities_that_were_never_in_scope() {
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

    // Verify entity exists on server before updating/removing
    scenario.expect(|ctx| ctx.server(|s| s.has_entity(&entity_e)).then_some(()));

    // Update and remove component while entity is not in A's scope
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 10.0;
                }
                e.remove_component::<Position>();
            }
        });
    });

    // Verify A never sees the entity
    scenario.expect(|ctx| (!ctx.client(client_a_key, |c| c.has_entity(&entity_e))).then_some(()));

    // TODO: Verify A sees no events for this entity
}

/// Client never sees update or insert events before seeing a spawn event
/// Contract: [client-events-05], [client-events-06], [client-events-07]
///
/// Given E is spawned then updated/extended; when A processes events;
/// then first event for E is spawn (plus possible initial inserts) and no update/remove is seen before spawn.
#[test]
fn client_never_sees_update_or_insert_events_before_seeing_a_spawn_event() {
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
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 10.0;
                }
            }
            (entity_e, local_entity)
        })
    });

    // Wait for entity to be visible
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // TODO: Verify that spawn event comes before update event
    // TODO: Verify no update/remove is seen before spawn
}

/// Client never sees events after despawn for a given entity
/// Contract: [client-events-08], [client-events-09]
///
/// Given E is spawned, updated, then despawned while in A's scope;
/// when A processes events after despawn, including under packet reordering;
/// then E generates no further events.
#[test]
fn client_never_sees_events_after_despawn_for_a_given_entity() {
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

    // Wait for entity to be visible
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

    scenario.expect(|_ctx| Some(()));

    // Despawn entity
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.despawn(&entity_e);
        });
    });

    // Wait for entity to be removed from client
    scenario.expect(|ctx| (!ctx.client(client_a_key, |c| c.has_entity(&entity_e))).then_some(()));

    // TODO: Verify no further events are generated for this entity
    // TODO: Test under packet reordering conditions
}

/// Client message events are grouped and typed correctly per channel
/// Contract: [client-events-10]
///
/// Given A receives multiple message types over multiple channels in one tick;
/// when A drains message events; then each message appears once with correct type and bound to correct channel.
#[test]
fn client_message_events_are_grouped_and_typed_correctly_per_channel() {
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

    // Send multiple messages on different channels
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(2));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(10));
        });
    });

    // Verify messages are grouped correctly
    scenario.expect(|ctx| {
        let reliable_messages: Vec<u32> = ctx.client(client_a_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });
        let ordered_messages: Vec<u32> = ctx.client(client_a_key, |c| {
            c.read_message::<OrderedChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });

        // ReliableChannel should have 1, 2
        let reliable_correct = reliable_messages.contains(&1) && reliable_messages.contains(&2);
        // OrderedChannel should have 10
        let ordered_correct = ordered_messages.contains(&10);
        // No cross-contamination
        let no_10_in_reliable = !reliable_messages.contains(&10);
        let no_1_in_ordered = !ordered_messages.contains(&1) && !ordered_messages.contains(&2);

        (reliable_correct && ordered_correct && no_10_in_reliable && no_1_in_ordered).then_some(())
    });
}

/// Client request/response events are drained once and matched correctly
/// Contract: [client-events-11], [client-events-12]
///
/// Given multiple server-to-client requests and client responses across ticks;
/// when client processes its request/response events; then each incoming request and outgoing response appears once,
/// is matchable to correct logical ID/handle, and does not reappear.
#[test]
fn client_request_response_events_are_drained_once_and_matched_correctly() {
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

    // Server sends multiple requests
    let (response_key_1, response_key_2) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let key_1 = server
                .send_request::<ReliableChannel, naia_test::test_protocol::TestRequest>(
                    &client_a_key,
                    &naia_test::test_protocol::TestRequest::new("query1"),
                )
                .expect("Failed to send request");
            let key_2 = server
                .send_request::<ReliableChannel, naia_test::test_protocol::TestRequest>(
                    &client_a_key,
                    &naia_test::test_protocol::TestRequest::new("query2"),
                )
                .expect("Failed to send request");
            (key_1, key_2)
        })
    });

    // Client receives and responds to requests
    let response_ids = scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            let mut ids = Vec::new();
            for (response_id, request) in
                c.read_request::<ReliableChannel, naia_test::test_protocol::TestRequest>()
            {
                ids.push((response_id, request.query));
            }
            if ids.len() == 2 {
                Some(ids)
            } else {
                None
            }
        })
    });

    // Client sends responses
    scenario.mutate(|ctx| {
        ctx.client(client_a_key, |c| {
            for (response_id, query) in &response_ids {
                let response_send_key = naia_shared::ResponseSendKey::new(*response_id);
                let result = format!("result_{}", query);
                c.send_response(
                    &response_send_key,
                    &naia_test::test_protocol::TestResponse::new(&result),
                );
            }
        });
    });

    scenario.expect(|_ctx| Some(()));

    // Verify server receives responses
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some((client_key, response)) = server.receive_response(&response_key_1) {
                assert_eq!(client_key, client_a_key);
                assert_eq!(response.result, "result_query1");
            } else {
                panic!("Expected response 1");
            }
            if let Some((client_key, response)) = server.receive_response(&response_key_2) {
                assert_eq!(client_key, client_a_key);
                assert_eq!(response.result, "result_query2");
            } else {
                panic!("Expected response 2");
            }
        });
    });

    // TODO: Verify each request/response appears exactly once
    // TODO: Verify they don't reappear on subsequent calls
}
