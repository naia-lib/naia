use naia_client::ClientConfig;
use naia_server::ServerConfig;
use naia_shared::Protocol;
use naia_test::{protocol, Auth, ClientKey, Scenario};

mod test_helpers;
use test_helpers::client_connect;

use naia_test::test_protocol::{OrderedChannel, ReliableChannel};
use naia_test::test_protocol::{Position, TestMessage};

// ============================================================================
// Domain 7: Protocol, Types, Serialization & Version Skew
// ============================================================================

/// Serialization failures are surfaced without poisoning the connection
///
/// Given a type that can be forced to fail (de)serialization; when such a failure occurs;
/// then side detecting error surfaces an appropriate error, ignores the failing message/entity,
/// and connection continues functioning for other traffic.
#[test]
fn serialization_failures_are_surfaced_without_poisoning_the_connection() {
    // TODO: This test requires a way to force serialization failures
    // This may require creating a custom message/component type that can fail serialization
    // or using a corrupted protocol definition
}

/// Multi-type mapping across messages, components, and channels
///
/// Given protocol with multiple message types on multiple channels and multiple component types;
/// when server/client exchange mixed messages and entity updates;
/// then each received message arrives as correct type on correct channel, each update as correct component type,
/// and nothing is misrouted/decoded as wrong type.
#[test]
fn multi_type_mapping_across_messages_components_and_channels() {
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

    // Spawn entity with Position component and include in both clients' scopes
    let (entity_e, _) = scenario.mutate(|ctx| {
        let entity = ctx.server(|server| {
            server.spawn(|mut e| {
                e.insert_component(Position::new(1.0, 2.0));
                e.enter_room(&room_key);
            })
        });
        ctx.server(|server| {
            server
                .user_scope_mut(&client_a_key)
                .unwrap()
                .include(&entity.0);
            server
                .user_scope_mut(&client_b_key)
                .unwrap()
                .include(&entity.0);
        });
        entity
    });

    // Wait for both clients to see the entity
    scenario.expect(|ctx| {
        let a_sees_e = ctx.client(client_a_key, |c| c.has_entity(&entity_e));
        let b_sees_e = ctx.client(client_b_key, |c| c.has_entity(&entity_e));
        (a_sees_e && b_sees_e).then_some(())
    });

    // Server sends different message types on different channels
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Send TestMessage on ReliableChannel to A
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(42));
            // Send TestMessage on OrderedChannel to B
            server.send_message::<OrderedChannel, _>(&client_b_key, &TestMessage::new(100));
        });
    });

    // Verify each client receives the correct message on the correct channel
    scenario.expect(|ctx| {
        let a_received: Vec<u32> = ctx.client(client_a_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });
        let b_received: Vec<u32> = ctx.client(client_b_key, |c| {
            c.read_message::<OrderedChannel, TestMessage>()
                .map(|m| m.value)
                .collect()
        });

        // A should receive 42 on ReliableChannel, B should receive 100 on OrderedChannel
        let a_correct = a_received.contains(&42);
        let b_correct = b_received.contains(&100);

        // Verify no cross-channel contamination
        let a_no_ordered = !ctx.client(client_a_key, |c| {
            c.read_message::<OrderedChannel, TestMessage>()
                .next()
                .is_some()
        });
        let b_no_reliable = !ctx.client(client_b_key, |c| {
            c.read_message::<ReliableChannel, TestMessage>()
                .next()
                .is_some()
        });

        (a_correct && b_correct && a_no_ordered && b_no_reliable).then_some(())
    });

    // Verify both clients see the Position component correctly
    scenario.expect(|ctx| {
        let a_has_pos = ctx.client(client_a_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().is_some()
            } else {
                false
            }
        });
        let b_has_pos = ctx.client(client_b_key, |c| {
            if let Some(e) = c.entity(&entity_e) {
                e.component::<Position>().is_some()
            } else {
                false
            }
        });
        (a_has_pos && b_has_pos).then_some(())
    });
}

/// Channel separation for different message types
///
/// Given messages bound to ChannelA vs ChannelB; when server sends A1,A2 on A and B1,B2 on B;
/// then client observes A1,A2 only through ChannelA API and B1,B2 only through ChannelB API.
#[test]
fn channel_separation_for_different_message_types() {
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

    // Server sends messages on different channels
    scenario.mutate(|ctx| {
        ctx.server(|server| {
            // Send A1, A2 on ReliableChannel
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(1));
            server.send_message::<ReliableChannel, _>(&client_a_key, &TestMessage::new(2));

            // Send B1, B2 on OrderedChannel
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(10));
            server.send_message::<OrderedChannel, _>(&client_a_key, &TestMessage::new(20));
        });
    });

    // Verify client receives messages on correct channels only
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
        // OrderedChannel should have 10, 20
        let ordered_correct = ordered_messages.contains(&10) && ordered_messages.contains(&20);

        // No cross-contamination
        let no_1_in_ordered = !ordered_messages.contains(&1) && !ordered_messages.contains(&2);
        let no_10_in_reliable =
            !reliable_messages.contains(&10) && !reliable_messages.contains(&20);

        (reliable_correct && ordered_correct && no_1_in_ordered && no_10_in_reliable).then_some(())
    });
}

/// Protocol type-order mismatch fails fast at handshake
///
/// Given server/client with intentionally mismatched protocol definitions (type ID ordering differs);
/// when client connects; then handshake fails early with clear mismatch outcome,
/// no gameplay events are generated, and both sides clean up.
#[test]
fn protocol_type_order_mismatch_fails_fast_at_handshake() {
    // TODO: This test requires creating mismatched protocol definitions
    // This may require modifying the protocol builder to allow custom type ordering
    // or creating a separate protocol builder for testing
}

/// Client missing a type that the server uses
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

/// Safe extension: server knows extra type but still interoperates
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

/// Schema incompatibility produces immediate, clear failure
///
/// Given server/client with incompatible schemas for a shared type; when they attempt to exchange that type;
/// then incompatibility is detected and surfaced as error/disconnect before corrupted values reach public API.
#[test]
fn schema_incompatibility_produces_immediate_clear_failure() {
    // TODO: This test requires creating incompatible schemas for the same type
    // This may require modifying the serialization format or field definitions
    // to create a schema mismatch that can be detected
}
