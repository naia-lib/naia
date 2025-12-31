use naia_client::ClientConfig;
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientKey, Scenario};

mod test_helpers;
use test_helpers::client_connect;

use naia_test::test_protocol::ReliableChannel;
use naia_test::test_protocol::{Position, TestMessage};

// ============================================================================
// Domain 9: Integration & Transport Parity
// ============================================================================

/// Core replication scenario behaves identically over UDP and WebRTC
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

/// Transport-specific connection failure surfaces cleanly
///
/// Given WebRTC transport configured so ICE/signalling fails; when client attempts to connect;
/// then connection eventually fails with clear error, no partial user/room state is created on server,
/// and client doesn't get stuck half-connected.
#[test]
fn transport_specific_connection_failure_surfaces_cleanly() {
    // TODO: This test requires WebRTC transport with configured failure conditions
    // The test harness currently uses LocalTransportHub which doesn't support transport-specific failures
}

/// Integrated "everything at once" scenario stays consistent and error-free
///
/// Given a complex scenario exercising all major features simultaneously (multiple clients, rooms, scoping,
/// entity replication with ownership/delegation, messages on multiple channels, requests/responses, tick-buffered commands);
/// when run to completion; then all features work correctly together, no errors occur, state remains consistent,
/// and no resource leaks are detected.
#[test]
fn integrated_everything_at_once_scenario_stays_consistent_and_error_free() {
    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let (room1_key, room2_key) = scenario.mutate(|ctx| {
        let r1 = ctx.server(|server| server.make_room().key());
        let r2 = ctx.server(|server| server.make_room().key());
        (r1, r2)
    });

    // Connect multiple clients
    let client_a_key = client_connect(
        &mut scenario,
        &room1_key,
        "Client A",
        Auth::new("client_a", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_b_key = client_connect(
        &mut scenario,
        &room1_key,
        "Client B",
        Auth::new("client_b", "password"),
        ClientConfig::default(),
        test_protocol.clone(),
    );
    let client_c_key = client_connect(
        &mut scenario,
        &room2_key,
        "Client C",
        Auth::new("client_c", "password"),
        ClientConfig::default(),
        test_protocol,
    );

    // Server spawns entities and include in different scopes
    let ((entity_e1, _), (entity_e2, _)) = scenario.mutate(|ctx| {
        let e1 = ctx.server(|server| {
            server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room1_key);
            })
        });
        let e2 = ctx.server(|server| {
            server.spawn(|mut e| {
                e.insert_component(Position::new(10.0, 20.0));
                e.enter_room(&room2_key);
            })
        });
        // Include entities in different scopes
        ctx.server(|server| {
            // E1 in A and B's scope (room1)
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&e1.0);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&e1.0);
            // E2 in C's scope (room2)
            server
                .user_scope_mut(&client_c_key)
                .unwrap()
                .include(&e2.0);
        });
        (e1, e2)
    });

    // Wait for entities to be visible
    scenario.expect(|ctx| {
        let a_sees_e1 = ctx.client(client_a_key, |c| c.has_entity(&entity_e1));
        let b_sees_e1 = ctx.client(client_b_key, |c| c.has_entity(&entity_e1));
        let c_sees_e2 = ctx.client(client_c_key, |c| c.has_entity(&entity_e2));
        (a_sees_e1 && b_sees_e1 && c_sees_e2).then_some(())
    });

    // Update entities
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            if let Some(mut e) = server.entity_mut(&entity_e1) {
                if let Some(mut pos) = e.component::<Position>() {
                    *pos.x = 100.0;
                }
            }
        });
    });

    scenario.expect(|_ctx| Some(()));

    // Send messages on different channels
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            server.send_message::<ReliableChannel, _>(&client_b_key, &TestMessage::new(2));
        });
    });

    // Verify messages received
    scenario.expect(|ctx| {
        let a_received = ctx.client(client_a_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .any(|m| m.value == 1)
        });
        let b_received = ctx.client(client_b_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .any(|m| m.value == 2)
        });
        (a_received && b_received).then_some(())
    });

    // Move client A to room2
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            let mut user_a = server.user_mut(&client_a_key).unwrap();
            user_a.leave_room(&room1_key);
            user_a.enter_room(&room2_key);
        });
    });

    // Verify A no longer sees E1 but can see E2
    scenario.expect(|ctx| {
        let a_no_e1 = !ctx.client(client_a_key, |c| c.has_entity(&entity_e1));
        let a_sees_e2 = ctx.client(client_a_key, |c| c.has_entity(&entity_e2));
        (a_no_e1 && a_sees_e2).then_some(())
    });

    // Despawn E1
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.despawn(&entity_e1);
        });
    });

    // Verify E1 is removed from B
    scenario.expect(|ctx| (!ctx.client(client_b_key, |c| c.has_entity(&entity_e1))).then_some(()));

    // Disconnect client B
    scenario.mutate(|ctx| {
        ctx.client(client_b_key, |c| {
            c.disconnect();
        });
    });

    // Wait for disconnect
    scenario.expect(|ctx| (!ctx.server(|s| s.user_exists(&client_b_key))).then_some(()));

    scenario.mutate(|_ctx| {});

    // Verify final state: A and C still connected, E2 still exists
    scenario.expect(|ctx| {
        let a_connected = ctx.server(|s| s.user_exists(&client_a_key));
        let c_connected = ctx.server(|s| s.user_exists(&client_c_key));
        let e2_exists = ctx.server(|s| s.has_entity(&entity_e2));
        (a_connected && c_connected && e2_exists).then_some(())
    });

    // TODO: Verify no resource leaks
    // TODO: Verify no errors occurred
}
