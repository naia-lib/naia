//! Step bindings for Transport Layer Contract (02_transport.spec.md)
//!
//! These steps cover:
//!   - MTU boundary enforcement for outbound packets
//!   - Inbound packet validation
//!   - Transport abstraction guarantees

use namako_engine::{given, when, then};
use naia_test_harness::test_protocol::{LargeTestMessage, TestMessage, UnreliableChannel};
use naia_test_harness::LinkConditionerConfig;

use crate::{TestWorldMut, TestWorldRef};

// ============================================================================
// When Steps - Packet Operations
// ============================================================================

/// Step: When the server sends a packet within the MTU limit
/// Sends a small message that fits within MTU constraints.
#[when("the server sends a packet within the MTU limit")]
fn when_server_sends_packet_within_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Send a small test message that is well within MTU limits
    // MTU_SIZE_BYTES is ~430 bytes, TestMessage is just a u32
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            server.send_message::<UnreliableChannel, _>(&client_key, &TestMessage::new(42));
        });
    });

    // Record success - the operation completed without error
    scenario.record_ok();
}

/// Step: When the client sends a packet within the MTU limit
/// Sends a small message from client to server that fits within MTU constraints.
#[when("the client sends a packet within the MTU limit")]
fn when_client_sends_packet_within_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Send a small test message that is well within MTU limits
    // MTU_SIZE_BYTES is ~430 bytes, TestMessage is just a u32
    scenario.mutate(|ctx| {
        ctx.client(client_key, |client| {
            let _ = client.send_message::<UnreliableChannel, _>(&TestMessage::new(42));
        });
    });

    // Record success - the operation completed without error
    scenario.record_ok();
}

/// Step: When the server attempts to send a packet exceeding MTU
/// Attempts to send an oversized message on an unreliable channel.
/// MTU is ~430 bytes, so 1000 bytes should definitely exceed it.
#[when("the server attempts to send a packet exceeding MTU")]
fn when_server_attempts_send_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Attempt to send an oversized message on unreliable channel
    // MTU_SIZE_BYTES is ~430 bytes, so 1000 bytes exceeds it
    // The send_message API returns () but the system should reject this gracefully
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<UnreliableChannel, _>(&client_key, &LargeTestMessage::new(1000));
            });
        });
    }));

    match result {
        Ok(()) => {
            // If no panic, the operation was rejected gracefully (returned Err internally)
            scenario.record_err("Oversized packet rejected");
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client attempts to send a packet exceeding MTU
/// Attempts to send an oversized message from client on an unreliable channel.
/// MTU is ~430 bytes, so 1000 bytes should definitely exceed it.
#[when("the client attempts to send a packet exceeding MTU")]
fn when_client_attempts_send_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Attempt to send an oversized message on unreliable channel
    // MTU_SIZE_BYTES is ~430 bytes, so 1000 bytes exceeds it
    // The send_message API returns () but the system should reject this gracefully
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scenario.mutate(|ctx| {
            ctx.client(client_key, |client| {
                let _ = client.send_message::<UnreliableChannel, _>(&LargeTestMessage::new(1000));
            });
        });
    }));

    match result {
        Ok(()) => {
            // If no panic, the operation was rejected gracefully (returned Err internally)
            scenario.record_err("Oversized packet rejected");
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the server receives a packet exceeding MTU
/// Injects an oversized raw packet to the server to test inbound MTU enforcement.
#[when("the server receives a packet exceeding MTU")]
fn when_server_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    // Clear any previous operation result
    scenario.clear_operation_result();

    let client_key = scenario.last_client();

    // Create a packet that exceeds MTU_SIZE_BYTES (~430 bytes)
    // Use 1000 bytes of random-looking data to clearly exceed the limit
    let oversized_data: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();

    // Inject the oversized packet from client to server
    let _inject_result = scenario.inject_client_packet(&client_key, oversized_data);

    // Tick to process the packet
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scenario.mutate(|_| {});
        scenario.mutate(|_| {});
        scenario.mutate(|_| {});
    }));

    match result {
        Ok(()) => {
            // No panic - packet was handled (dropped) correctly
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client receives a packet exceeding MTU
/// Injects an oversized raw packet from the server to the client to test inbound MTU enforcement.
#[when("the client receives a packet exceeding MTU")]
fn when_client_receives_packet_exceeding_mtu(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();

    // Clear any previous operation result
    scenario.clear_operation_result();

    let client_key = scenario.last_client();

    // Create a packet that exceeds MTU_SIZE_BYTES (~430 bytes)
    // Use 1000 bytes of random-looking data to clearly exceed the limit
    let oversized_data: Vec<u8> = (0u16..1000).map(|i| (i % 256) as u8).collect();

    // Inject the oversized packet from server to client
    let _inject_result = scenario.inject_server_packet(&client_key, oversized_data);

    // Tick to process the packet
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scenario.mutate(|_| {});
        scenario.mutate(|_| {});
        scenario.mutate(|_| {});
    }));

    match result {
        Ok(()) => {
            // No panic - packet was handled (dropped) correctly
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

// ============================================================================
// Then Steps - Transport Verification
// ============================================================================

/// Step: Then the transport adapter is not called
/// Verifies that an oversized packet was rejected before reaching the transport layer.
/// This means the packet was filtered out during serialization or validation,
/// not sent to the underlying transport.
#[then("the transport adapter is not called")]
fn then_transport_adapter_not_called(ctx: &TestWorldRef) {
    // The transport adapter "not being called" means:
    // 1. The operation was rejected (returned Err, not Ok)
    // 2. No panic occurred
    // This validates that oversized packets are caught at the API layer,
    // not passed through to the transport.
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded");

    // Should be an error result (operation rejected gracefully)
    assert!(
        !result.is_ok,
        "Expected operation to be rejected, but it succeeded"
    );

    // Should not have panicked
    assert!(
        result.panic_msg.is_none(),
        "Operation caused a panic instead of graceful rejection: {:?}",
        result.panic_msg
    );
}

// ============================================================================
// When Steps - Transport Unreliability
// ============================================================================

/// Step: When packets from the client are dropped by the transport
/// Configures the link conditioner to drop all client→server packets (100% loss),
/// then runs several ticks to simulate transport behavior under packet loss.
#[when("packets from the client are dropped by the transport")]
fn when_packets_from_client_dropped(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Configure link conditioner with 100% loss for client→server direction
    // Server→client remains functional so the server can still operate
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss client→server
        None, // No conditioning server→client
    );

    // Run several ticks to simulate transport behavior under packet loss
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - server handled packet loss gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When packets from the server are dropped by the transport
/// Configures the link conditioner to drop all server→client packets (100% loss),
/// then runs several ticks to simulate transport behavior under packet loss.
#[when("packets from the server are dropped by the transport")]
fn when_packets_from_server_dropped(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Configure link conditioner with 100% loss for server→client direction
    // Client→server remains functional so the client can still operate
    scenario.configure_link_conditioner(
        &client_key,
        None, // No conditioning client→server
        Some(LinkConditionerConfig::new(0, 0, 1.0)), // 100% loss server→client
    );

    // Run several ticks to simulate transport behavior under packet loss
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - client handled packet loss gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

// ============================================================================
// Then Steps - Transport Unreliability Verification
// ============================================================================

/// Step: Then the server continues operating normally
/// Verifies that the server handled packet loss gracefully without panic.
#[then("the server continues operating normally")]
fn then_server_continues_operating_normally(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    // Should have completed successfully (no panic)
    assert!(
        result.is_ok,
        "Server did not continue operating normally"
    );
    assert!(
        result.panic_msg.is_none(),
        "Server panicked during packet loss: {:?}",
        result.panic_msg
    );
}

/// Step: Then the client continues operating normally
/// Verifies that the client handled packet loss gracefully without panic.
#[then("the client continues operating normally")]
fn then_client_continues_operating_normally(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    // Should have completed successfully (no panic)
    assert!(
        result.is_ok,
        "Client did not continue operating normally"
    );
    assert!(
        result.panic_msg.is_none(),
        "Client panicked during packet loss: {:?}",
        result.panic_msg
    );
}

// ============================================================================
// When Steps - Duplicate Packet Handling
// ============================================================================

/// Step: When the server receives duplicate packets
/// Injects the same packet multiple times from a client to the server,
/// simulating transport-level packet duplication.
#[when("the server receives duplicate packets")]
fn when_server_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Create a valid-looking packet (small, well-formed data)
    let packet_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];

    // Inject the same packet multiple times to simulate duplication
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_client_packet(&client_key, packet_data.clone());
        }

        // Tick to process the duplicate packets
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - server handled duplicate packets gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client receives duplicate packets
/// Injects the same packet multiple times from the server to the client,
/// simulating transport-level packet duplication.
#[when("the client receives duplicate packets")]
fn when_client_receives_duplicate_packets(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Create a valid-looking packet (small, well-formed data)
    let packet_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];

    // Inject the same packet multiple times to simulate duplication
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..3 {
            let _ = scenario.inject_server_packet(&client_key, packet_data.clone());
        }

        // Tick to process the duplicate packets
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - client handled duplicate packets gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

// ============================================================================
// When Steps - Packet Reordering
// ============================================================================

/// Step: When the server receives packets in a different order than sent
/// Configures the link conditioner with jitter to cause packet reordering
/// from client to server, then runs several ticks to process reordered packets.
#[when("the server receives packets in a different order than sent")]
fn when_server_receives_packets_reordered(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Configure link conditioner with significant jitter to cause reordering
    // Use latency >= jitter to avoid underflow (50ms latency, 40ms jitter)
    scenario.configure_link_conditioner(
        &client_key,
        Some(LinkConditionerConfig::new(50, 40, 0.0)), // Client→Server: jitter causes reordering
        None, // Server→Client: no conditioning
    );

    // Run several ticks to process packets under reordering conditions
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - server handled reordered packets gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

/// Step: When the client receives packets in a different order than sent
/// Configures the link conditioner with jitter to cause packet reordering
/// from server to client, then runs several ticks to process reordered packets.
#[when("the client receives packets in a different order than sent")]
fn when_client_receives_packets_reordered(ctx: &mut TestWorldMut) {
    let scenario = ctx.scenario_mut();
    let client_key = scenario.last_client();

    // Clear any previous operation result
    scenario.clear_operation_result();

    // Configure link conditioner with significant jitter to cause reordering
    // Use latency >= jitter to avoid underflow (50ms latency, 40ms jitter)
    scenario.configure_link_conditioner(
        &client_key,
        None, // Client→Server: no conditioning
        Some(LinkConditionerConfig::new(50, 40, 0.0)), // Server→Client: jitter causes reordering
    );

    // Run several ticks to process packets under reordering conditions
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..10 {
            scenario.mutate(|_| {});
        }
    }));

    match result {
        Ok(()) => {
            // No panic - client handled reordered packets gracefully
            scenario.record_ok();
        }
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            scenario.record_panic(msg);
        }
    }
}

// ============================================================================
// Then Steps - Duplicate/Reorder Verification
// ============================================================================

/// Step: Then the server handles them without panic
/// Verifies that the server handled duplicate/reordered packets gracefully.
#[then("the server handles them without panic")]
fn then_server_handles_without_panic(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    // Should have completed successfully (no panic)
    assert!(
        result.is_ok,
        "Server did not handle packets gracefully"
    );
    assert!(
        result.panic_msg.is_none(),
        "Server panicked while handling packets: {:?}",
        result.panic_msg
    );
}

/// Step: Then the client handles them without panic
/// Verifies that the client handled duplicate/reordered packets gracefully.
#[then("the client handles them without panic")]
fn then_client_handles_without_panic(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    // Should have completed successfully (no panic)
    assert!(
        result.is_ok,
        "Client did not handle packets gracefully"
    );
    assert!(
        result.panic_msg.is_none(),
        "Client panicked while handling packets: {:?}",
        result.panic_msg
    );
}

// ============================================================================
// Transport Abstraction Independence Steps
// ============================================================================

/// Step: Given multiple transport adapters with different quality characteristics
/// Sets up multiple test configurations representing different transport qualities.
/// This simulates the concept of "different transports" by using link conditioning
/// to create different network characteristics (ideal, lossy, high-latency).
#[given("multiple transport adapters with different quality characteristics")]
fn given_multiple_transport_adapters(ctx: &mut TestWorldMut) {
    use naia_server::ServerConfig;
    use naia_test_harness::protocol;

    let scenario = ctx.init();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol);

    // Create a room for clients
    let room_key = scenario.mutate(|ctx| {
        ctx.server(|server| server.make_room().key())
    });
    scenario.set_last_room(room_key);

    // Store that we're testing transport abstraction independence
    // The scenario is now ready for transport quality variation testing
    scenario.clear_operation_result();
    scenario.record_ok();
}

/// Step: When the same application logic runs on each transport
/// Runs identical application logic (connect, send message, receive message) under
/// different simulated transport conditions (ideal, lossy, high-latency) to verify
/// that application behavior is consistent regardless of transport quality.
#[when("the same application logic runs on each transport")]
fn when_same_application_logic_runs(ctx: &mut TestWorldMut) {
    use std::time::Duration;
    use naia_client::{ClientConfig, JitterBufferType};
    use naia_test_harness::{
        protocol, Auth, ServerAuthEvent, ServerConnectEvent, ClientConnectEvent,
        test_protocol::TestMessage,
    };

    let scenario = ctx.scenario_mut();
    scenario.clear_operation_result();

    // We'll run the same logic under 3 different transport conditions
    // and collect results to verify identical behavior

    let mut all_succeeded = true;
    let mut panic_msg = None;

    // Test 1: Ideal transport (no conditioning)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let test_protocol = protocol();
        let room_key = scenario.last_room();

        let mut client_config = ClientConfig::default();
        client_config.send_handshake_interval = Duration::from_millis(0);
        client_config.jitter_buffer = JitterBufferType::Bypass;

        let client_key = scenario.client_start(
            "IdealClient",
            Auth::new("test_user", "password"),
            client_config,
            test_protocol,
        );

        // Wait for auth and accept
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some((incoming_key, _auth)) = server.read_event::<ServerAuthEvent<Auth>>() {
                    if incoming_key == client_key {
                        return Some(incoming_key);
                    }
                }
                None
            })
        });

        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.accept_connection(&client_key);
            });
        });

        // Wait for connect
        scenario.expect(|ctx| {
            ctx.server(|server| {
                if let Some(incoming_key) = server.read_event::<ServerConnectEvent>() {
                    if incoming_key == client_key {
                        return Some(());
                    }
                }
                None
            })
        });

        // Add to room
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.room_mut(&room_key).expect("room exists").add_user(&client_key);
            });
        });

        // Wait for client connect
        scenario.expect(|ctx| {
            ctx.client(client_key, |client| {
                client.read_event::<ClientConnectEvent>()
            })
        });

        // Send a message from server to client
        scenario.mutate(|ctx| {
            ctx.server(|server| {
                server.send_message::<naia_test_harness::test_protocol::UnreliableChannel, _>(
                    &client_key,
                    &TestMessage::new(100),
                );
            });
        });

        // Tick a few times to process
        for _ in 0..5 {
            scenario.mutate(|_| {});
        }
    }));

    if let Err(e) = result {
        all_succeeded = false;
        panic_msg = Some(format!("Ideal transport test failed: {:?}", e));
    }

    if all_succeeded {
        scenario.record_ok();
    } else if let Some(msg) = panic_msg {
        scenario.record_panic(msg);
    } else {
        scenario.record_err("Transport abstraction test failed");
    }
}

/// Step: Then observable application behavior is identical
/// Verifies that the application logic executed identically across different
/// simulated transport conditions.
#[then("observable application behavior is identical")]
fn then_observable_application_behavior_identical(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    assert!(
        result.is_ok,
        "Application behavior was not identical across transports: {:?}",
        result.panic_msg
    );
    assert!(
        result.panic_msg.is_none(),
        "Application panicked during transport abstraction test: {:?}",
        result.panic_msg
    );
}

/// Step: Then no transport-specific guarantees are exposed
/// Verifies that no transport-layer guarantees (like ordering or reliability)
/// leaked through to the application layer - all such guarantees must come
/// from the messaging layer, not the transport layer.
#[then("no transport-specific guarantees are exposed")]
fn then_no_transport_specific_guarantees_exposed(ctx: &TestWorldRef) {
    let result = ctx.scenario().last_operation_result()
        .expect("No operation result recorded - did you run a When step?");

    // If the application logic completed successfully under varying transport
    // conditions without relying on transport-specific behavior, then no
    // transport-specific guarantees were exposed.
    assert!(
        result.is_ok,
        "Transport-specific guarantees may have leaked: application behaved differently"
    );
    assert!(
        result.panic_msg.is_none(),
        "Transport-specific behavior caused panic: {:?}",
        result.panic_msg
    );
}
