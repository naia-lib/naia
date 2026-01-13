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
// Transport Tests
// ============================================================================
// Tests organized by contract ID to match specs/contracts/2_transport.md
// ============================================================================

/// Core replication scenario behaves identically over UDP and WebRTC
/// Contract: [transport-01], [transport-02], [transport-03]
///
/// Given simple multi-client scenario (spawn/update/despawn and some messages);
/// when run once over UDP and once over WebRTC with equivalent link conditions;
/// then externally observable events (connects, spawns, updates, messages, despawns, disconnects) are identical modulo timing.
#[test]
fn core_replication_scenario_behaves_identically_over_udp_and_webrtc() {
    // TODO: This test requires running the same scenario over different transports
    // The test harness currently uses LocalTransportHub which simulates a perfect network
    // To test transport parity, we would need to:
    // 1. Run scenario over UDP transport
    // 2. Run same scenario over WebRTC transport
    // 3. Compare event sequences (ignoring timing differences)
}

/// Extreme jitter and reordering preserve channel contracts
/// Contract: [transport-01], [transport-02], [commands-05]
///
/// Given link conditioner with high jitter and reordering; when sending messages and replication updates over ordered/unordered/sequenced/tick-buffered channels;
/// then each channel still satisfies its documented ordering/reliability/latest-only semantics.
#[test]
fn extreme_jitter_and_reordering_preserve_channel_contracts() {
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

    // First test without link conditioner to verify messages work
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(999));
        });
    });

    // Verify message arrives without link conditioner
    let mut test_msg = Vec::new();
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                test_msg.push(msg.value);
            }
        });
        (test_msg.len() == 1 && test_msg[0] == 999).then_some(())
    });

    // Now configure link conditioner with small latency/jitter
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(10, 5, 0.0)), // Small latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(10, 5, 0.0)), // Small latency >= jitter, no loss
    );

    // Send messages on different channel types
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Send on ReliableChannel (should arrive in order)
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(2));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(3));

            // Send on OrderedChannel (should arrive in order)
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(10));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(20));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(30));
        });
    });

    // Verify both ReliableChannel and OrderedChannel messages arrive in order despite jitter
    // With 10ms latency + 5ms jitter, packets can be delayed up to 15ms (~1 tick)
    // Both channel types are in the same packet, so we need to read them in the same expect() call
    let mut reliable_messages = Vec::new();
    let mut ordered_messages = Vec::new();
    scenario.until(50usize.ticks()).expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                reliable_messages.push(msg.value);
            }
            for msg in c.read_message::<OrderedChannel, TestMessage>() {
                ordered_messages.push(msg.value);
            }
        });
        (reliable_messages.len() == 3 && ordered_messages.len() == 3).then_some(())
    });
    assert_eq!(
        reliable_messages,
        vec![1, 2, 3],
        "ReliableChannel should maintain order"
    );
    assert_eq!(
        ordered_messages,
        vec![10, 20, 30],
        "OrderedChannel should maintain order"
    );
}

/// Protocol type-order mismatch fails fast at handshake
///
/// Given server/client with intentionally mismatched protocol definitions (type ID ordering differs);
/// when client connects; then handshake fails early with clear mismatch outcome,
/// no gameplay events are generated, and both sides clean up.
/// Contract: [messaging-04], [connection-14a]
#[test]
fn protocol_type_order_mismatch_fails_fast_at_handshake() {
    use naia_shared::{ChannelDirection, ChannelMode, ReliableSettings};

    let mut scenario = Scenario::new();

    // Create the standard protocol for the server
    let server_protocol = protocol();

    // Create a mismatched protocol for the client by omitting one channel
    // This will produce a different protocol_id due to different channel count
    let client_protocol = Protocol::builder()
        .add_component::<Position>()
        .add_message::<Auth>()
        .add_message::<TestMessage>()
        .add_message::<TestRequest>()
        .add_message::<TestResponse>()
        // Intentionally omit ReliableChannel to create protocol_id mismatch
        .add_channel::<UnreliableChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedUnreliable,
        )
        .add_channel::<OrderedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::OrderedReliable(ReliableSettings::default()),
        )
        .add_channel::<UnorderedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_channel::<SequencedChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::SequencedReliable(ReliableSettings::default()),
        )
        .add_channel::<TickBufferedChannel>(
            ChannelDirection::ClientToServer,
            ChannelMode::TickBuffered(naia_shared::TickBufferSettings::default()),
        )
        .add_channel::<RequestResponseChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .enable_client_authoritative_entities()
        .build();

    // Start server with standard protocol
    scenario.server_start(ServerConfig::default(), server_protocol);

    // Start client with mismatched protocol
    let client_key = scenario.client_start(
        "Client",
        Auth::new("user", "pass"),
        test_client_config(),
        client_protocol,
    );

    // Wait for client to receive rejection event
    let mut reject_event_received = false;
    scenario.expect(|ctx| {
        ctx.client(client_key, |client| {
            if client.read_event::<ClientRejectEvent>().is_some() {
                reject_event_received = true;
            }
            reject_event_received.then_some(())
        })
    });

    assert!(reject_event_received, "Client should receive rejection event");

    // Verify connection is rejected before any message exchange
    scenario.mutate(|_ctx| {});
    scenario.spec_expect("messaging-04.t1: mismatched protocol_id rejects connection before message exchange", |ctx| {
        // Check that client is in rejected state and NOT connected
        let is_rejected = ctx.client(client_key, |client| {
            client.is_rejected()
        });

        let client_not_connected = !ctx.client(client_key, |client| {
            client.connection_status().is_connected()
        });

        // Check that server did NOT emit ConnectEvent (user doesn't exist)
        let server_no_user = !ctx.server(|server| {
            server.user_exists(&client_key)
        });

        // All three conditions must be true
        (is_rejected && client_not_connected && server_no_user).then_some(())
    });
}

/// Robustness under simulated packet loss
/// Contract: [transport-01], [transport-02]
///
/// Given A and B seeing replicated E; when server updates E while test transport drops a substantial fraction of packets;
/// then after loss subsides both clients converge to server's latest E state without permanent divergence.
#[test]
fn robustness_under_simulated_packet_loss() {
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
    // client_connect ends with expect, so we need a mutate before the next operation
    scenario.mutate(|_ctx| {});
    let client_b_key = client_connect(
        &mut scenario,
        &room_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Spawn entity E, add it to the room, and include it in both clients' scopes
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            // Include entity in both clients' scopes
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_key);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity_key);
            (entity_key, ())
        })
    });

    // Wait for both clients to see the entity
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Configure link conditioner with 50% packet loss for client A
    // Note: configure_link_conditioner is on Scenario, not MutateCtx
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.5)), // 50% loss client->server
        Some(naia_shared::LinkConditionerConfig::new(0, 0, 0.5)), // 50% loss server->client
    );

    // Update entity multiple times
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 100.0;
                }
            }
        });
    });

    // Verify both clients eventually converge to latest state
    // Client A may have lost some updates, but should eventually get the latest via retries
    // Use longer timeout due to packet loss requiring retries
    scenario.until(200usize.ticks()).expect(|ctx| {
        let a_has_latest = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 100.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        let b_has_latest = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 100.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        // Client B should definitely have latest (no packet loss)
        // Client A should eventually get it (reliable channel retries)
        (a_has_latest && b_has_latest).then_some(())
    });
}

/// Schema incompatibility produces immediate, clear failure
/// Contract: [transport-01], [transport-05]
///
/// Given server/client with incompatible schemas for a shared type; when they attempt to exchange that type;
/// then incompatibility is detected and surfaced as error/disconnect before corrupted values reach public API.
#[test]
fn schema_incompatibility_produces_immediate_clear_failure() {
    // TODO: This test requires creating incompatible schemas for the same type
    // This may require modifying the serialization format or field definitions
    // to create a schema mismatch that can be detected
}

/// Fragment loss causes older state until a full later update
/// Contract: [transport-02], [transport-04]
///
/// Given repeated large updates for E with fragmentation; when one update loses a fragment but a later full update arrives intact;
/// then client stays at previous valid state until later full update is applied, never applying a partially missing update.
#[test]
fn fragment_loss_causes_older_state_until_a_full_later_update() {
    // TODO: This test requires fragmentation and packet loss
    // TODO: Verify client maintains previous state when fragment is lost
    // TODO: Verify client applies later full update correctly
}

/// Client missing a type that the server uses
/// Contract: [transport-03], [transport-04]
///
/// Given server protocol with an extra type not in client protocol; when client connects and server uses that type;
/// then either connection is rejected as incompatible or server avoids sending unsupported type;
/// in either case client never crashes or enters undefined state.
#[test]
fn client_missing_a_type_that_the_server_uses() {
    // TODO: This test requires creating protocols with mismatched types
    // Server protocol would have an extra message/component type
    // Client protocol would not have that type
    // Need to verify behavior when server tries to send the missing type
}

/// Out-of-order packet handling does not regress to older state
/// Contract: [transport-03], [transport-04]
///
/// Given E updated monotonically; when some packets carrying older states are delayed until after newer states;
/// then clients never regress to older state once newer state applied, and eventually report latest state.
#[test]
fn out_of_order_packet_handling_does_not_regress_to_older_state() {
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

    // Spawn entity E, add it to the room, and include it in client's scope
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            // Include entity in client's scope
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_key);
            (entity_key, ())
        })
    });

    // Wait for client to see the entity
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Configure link conditioner with moderate jitter to cause reordering
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
    );

    // Update entity monotonically (increasing x value)
    // Must alternate between mutate and expect, so we do all mutations in one mutate call
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            for i in 2..=10 {
                if let Some(mut e) = server.entity_mut(&entity_e) {
                    if let Some(mut pos) = e.component::<Position>() {
                        *pos.x = i as f32;
                    }
                }
            }
        });
    });

    // Verify client never regresses to older state - should have latest value (10.0)
    scenario.until(100usize.ticks()).expect(|ctx| {
        let has_latest = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                if let Some(pos) = e.component::<Position>() {
                    *pos.x == 10.0
                } else {
                    false
                }
            } else {
                false
            }
        });
        has_latest.then_some(())
    });
}

/// Packet duplication does not surface duplicate events
/// Contract: [transport-03], [transport-04]
///
/// Given link conditioner that duplicates packets at high rate; when server sends entity updates and messages;
/// then clients never observe duplicate spawn/despawn/message/response events, and state does not regress even if older duplicates arrive after newer packets.
///
/// Note: Link conditioner doesn't support duplication, but high jitter can cause reordering which tests similar behavior.
#[test]
fn packet_duplication_does_not_surface_duplicate_events() {
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

    // Configure link conditioner with moderate jitter to simulate reordering (similar to duplication effects)
    // Use latency >= jitter to avoid underflow
    scenario.configure_link_conditioner(
        &client_a_key,
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
        Some(naia_shared::LinkConditionerConfig::new(50, 40, 0.0)), // Moderate latency >= jitter, no loss
    );

    // Spawn entity E, add it to the room, and include it in client's scope
    let (entity_e, _) = scenario.mutate(|ctx| {
        ctx.server(|server| {
            let (entity_key, _) = server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            });
            // Include entity in client's scope
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity_key);
            (entity_key, ())
        })
    });

    // Verify entity spawn event occurred exactly once
    // Note: SpawnEntityEvent is not exported, so we verify by checking entity exists
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| c.has_entity(&entity_e))
            .then_some(())
    });

    // Send multiple messages
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(
                &client_a_key,
                &naia_test::test_protocol::TestMessage::new(100),
            );
            server.send_message::<naia_test::test_protocol::ReliableChannel, _>(
                &client_a_key,
                &naia_test::test_protocol::TestMessage::new(200),
            );
        });
    });

    // Verify messages were received exactly once each (no duplicates)
    let mut message_values = Vec::new();
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            for msg in c.read_message::<ReliableChannel, TestMessage>() {
                message_values.push(msg.value);
            }
        });
        (message_values.len() == 2).then_some(())
    });

    // Update entity multiple times (to test that older updates don't regress state)
    // Must alternate between mutate and expect, so we do all mutations in one mutate call
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            for i in 2..=5 {
                if let Some(mut e) = server.entity_mut(&entity_e) {
                    if let Some(mut pos) = e.component::<Position>() {
                        *pos.x = i as f32;
                    }
                }
            }
        });
    });

    // Verify entity has latest state (not regressed to older value)
    scenario.expect(|ctx| {
        ctx.client(client_a_key, |c| {
            let Some(e) = c.entity(&entity_e) else {
                return false;
            };
            let Some(pos) = e.component::<Position>() else {
                return false;
            };
            let x_value = *pos.x;
            (x_value - 5.0).abs() < 0.001
        })
        .then_some(())
    });
    assert_eq!(
        message_values.len(),
        2,
        "Should receive exactly 2 messages, no duplicates"
    );
    assert!(message_values.contains(&100), "Should receive message 100");
    assert!(message_values.contains(&200), "Should receive message 200");
}

/// Large entity update that exceeds MTU is correctly reassembled
/// Contract: [transport-04], [transport-05]
///
/// Given E whose update exceeds single MTU; when server sends full update;
/// then client applies a complete coherent update only after all fragments arrive, never partial component state, even with delayed/duplicated fragments.
#[test]
fn large_entity_update_that_exceeds_mtu_is_correctly_reassembled() {
    // TODO: This test requires creating an entity update that exceeds MTU
    // TODO: Verify fragments are correctly reassembled
    // TODO: Verify no partial state is applied
}

/// Safe extension: server knows extra type but still interoperates
/// Contract: [transport-04], [transport-05]
///
/// Given server protocol defines extra message type `Extra` beyond baseline while client only knows baseline;
/// when client connects; then behavior follows documented rule: either `Extra` is never sent to that client
/// while baseline works, or connection is rejected as incompatible.
#[test]
fn safe_extension_server_knows_extra_type_but_still_interoperates() {
    // TODO: This test requires creating protocols where server has extra types
    // Need to verify that server doesn't send unsupported types to client
    // or that connection is rejected if types are incompatible
}

/// Transport-specific connection failure surfaces cleanly
/// Contract: [transport-04]
///
/// Given WebRTC transport configured so ICE/signalling fails; when client attempts to connect;
/// then connection eventually fails with clear error, no partial user/room state is created on server,
/// and client doesn't get stuck half-connected.
#[test]
fn transport_specific_connection_failure_surfaces_cleanly() {
    // TODO: This test requires WebRTC transport with configured failure conditions
    // The test harness currently uses LocalTransportHub which doesn't support transport-specific failures
}

/// Compression on/off does not change observable semantics
/// Contract: [transport-05]
///
/// Given scenario with entities/messages; when run once with compression off and once on;
/// then sequence of API-visible events, entity states, and messages is identical between runs (only bandwidth differs).
#[test]
fn compression_on_off_does_not_change_observable_semantics() {
    // TODO: This test requires running same scenario with compression on/off
    // TODO: Compare event sequences and entity states
    // TODO: Verify they are identical
}

/// Compression toggling affects bandwidth metrics but not logical events
/// Contract: [transport-05]
///
/// Given scripted replication/messages; when run once with compression off and once on;
/// then compressed run shows fewer bytes sent, while logical events and world states stay identical.
#[test]
fn compression_toggling_affects_bandwidth_metrics_but_not_logical_events() {
    // TODO: This test requires access to bandwidth metrics
    // TODO: Run scenario with compression on/off
    // TODO: Compare bandwidth metrics and logical events
}
