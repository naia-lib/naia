//! Unit tests for the `pipeline_actors` packaging module.
//!
//! Phase A.1 (MISSION_SIM_OWNS_WORLD.md): verify that
//! [`spawn_server_handles`] constructs the three handles cleanly and
//! that each handle is `Send` (cyberlith needs to move them across
//! SubApp thread boundaries).
//!
//! Phase A.2: verify that [`drain_lifecycle`] translates
//! [`WorldEvents`]-carried Connect/Disconnect/Error entries into
//! [`RecvLifecycleEvent`] variants in order, and (under the `test_utils`
//! feature, which the workspace test build enables via feature
//! unification) that [`drain_tick_buffer`] returns injected
//! tick-buffered messages.

use std::collections::HashSet;
use std::net::SocketAddr;

use naia_shared::{BigMapKey, DisconnectReason, Protocol};

use crate::events::world_events::WorldEvents;
use crate::server::receive_output::ReceiveOutput;
use crate::{NaiaServerError, RecvHandle, SendHandle, ServerConfig};
use crate::user::UserKey;

use super::{
    CoordHandle, RecvLifecycleEvent, drain_lifecycle, drain_tick_buffer,
    spawn_server_handles,
};

/// Compile-time assertion that `T: Send`.
fn assert_send<T: Send>() {}

#[test]
fn spawn_server_handles_constructs_three_handles() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (coord, recv, send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

    // The three handles construct and own their own state. The Arc on
    // CoordHandle::shared is the same allocation as on the recv/send
    // handles, so the strong count must be at least 3.
    let strong = std::sync::Arc::strong_count(&coord.shared);
    assert!(
        strong >= 3,
        "expected at least 3 strong refs to ServerShared after split, got {strong}",
    );

    drop((coord, recv, send));
}

#[test]
fn pipeline_handles_are_send() {
    assert_send::<CoordHandle<u64>>();
    assert_send::<RecvHandle<u64>>();
    assert_send::<SendHandle<u64>>();
    // Naia-server doesn't depend on bevy_ecs, so we can't reference
    // `bevy_ecs::entity::Entity` here directly. Cyberlith's
    // `GameCell::init` instantiates `<E = bevy_ecs::entity::Entity>` —
    // that crate-level Send check happens at cyberlith-side compile time
    // via the generic bounds on `spawn_server_handles`.
}

fn make_empty_receive_output() -> ReceiveOutput<u64> {
    ReceiveOutput {
        world_events: WorldEvents::<u64>::new(),
        pending_ticks: Vec::new(),
        received_addresses: HashSet::new(),
        pending_data_packets: Vec::new(),
    }
}

#[test]
fn drain_lifecycle_on_empty_output_returns_empty_vec() {
    let mut output = make_empty_receive_output();
    let events = drain_lifecycle(&mut output);
    assert!(events.is_empty(), "no lifecycle events expected: {events:?}");
}

#[test]
fn drain_lifecycle_translates_connect_disconnect_error() {
    let mut output = make_empty_receive_output();

    let user_a = UserKey::from_u64(1);
    let user_b = UserKey::from_u64(2);
    let addr_b: SocketAddr = "127.0.0.1:9001".parse().unwrap();

    // Push entries directly into the recv-side WorldEvents accumulator
    // (pub(crate) APIs reachable from in-crate tests). This mirrors what
    // the recv loop does when handshake completion / disconnect / decode
    // error fires.
    output.world_events.push_connection(&user_a);
    output
        .world_events
        .push_disconnection(&user_b, addr_b, DisconnectReason::ClientDisconnected);
    output
        .world_events
        .push_error(NaiaServerError::Wrapped(Box::new(
            std::io::Error::new(std::io::ErrorKind::Other, "boom"),
        )));

    let events = drain_lifecycle(&mut output);
    assert_eq!(events.len(), 3, "expected 3 lifecycle events: {events:?}");

    match &events[0] {
        RecvLifecycleEvent::Connected { user_key } => assert_eq!(*user_key, user_a),
        other => panic!("expected Connected, got {other:?}"),
    }
    match &events[1] {
        RecvLifecycleEvent::Disconnected { user_key, address, reason } => {
            assert_eq!(*user_key, user_b);
            assert_eq!(*address, addr_b);
            assert!(matches!(reason, DisconnectReason::ClientDisconnected));
        }
        other => panic!("expected Disconnected, got {other:?}"),
    }
    match &events[2] {
        RecvLifecycleEvent::RecvError { error } => {
            // Just smoke-check the variant matches; the underlying error
            // type doesn't implement PartialEq.
            let _ = error;
        }
        other => panic!("expected RecvError, got {other:?}"),
    }

    // After drain, the WorldEvents lists should be empty (mem::take semantics).
    let leftover = drain_lifecycle(&mut output);
    assert!(leftover.is_empty(), "second drain should be empty: {leftover:?}");
}

#[test]
fn drain_tick_buffer_on_empty_recv_handle_is_empty() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (_coord, mut recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

    let messages = drain_tick_buffer(&mut recv, 0);
    // TickBufferMessages has no public is_empty / len; smoke-check that
    // the call completes and the value can be dropped cleanly.
    drop(messages);
}

// Test-local channel + message types used by the `test_utils`-gated tick
// buffer test below. The `Message` and `Channel` derive macros require
// the types to be at module scope (the derives expand into impls that
// can't sit inside a function body).
#[cfg(feature = "test_utils")]
mod tick_buffer_test_protocol {
    use naia_shared::{Channel, Message};

    #[derive(Channel)]
    pub struct TestTickBufferedChannel;

    #[derive(Message, PartialEq, Eq, Hash)]
    pub struct TestTick {
        pub value: u32,
    }
}

#[cfg(feature = "test_utils")]
#[test]
fn drain_tick_buffer_returns_injected_message_under_test_utils() {
    use naia_shared::{
        ChannelDirection, ChannelKind, ChannelMode, Message, MessageContainer,
        TickBufferSettings,
    };

    use tick_buffer_test_protocol::{TestTick, TestTickBufferedChannel};

    let mut proto = Protocol::builder();
    proto
        .add_channel::<TestTickBufferedChannel>(
            ChannelDirection::ClientToServer,
            ChannelMode::TickBuffered(TickBufferSettings::default()),
        )
        .add_message::<TestTick>();
    proto.lock();
    let protocol = proto.build();

    let (_coord, mut recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol);

    // Sanity-check that the channel kind made it into the shared
    // channel_kinds table — if this is empty, the protocol-build path is
    // broken (and inject_message below would fall through the `else`
    // arm and return false silently).
    let registered_channels = recv.state.shared.channel_kinds.all_names();
    assert!(
        registered_channels.iter().any(|n| n.contains("TestTickBufferedChannel")),
        "channel_kinds should contain TestTickBufferedChannel, saw {registered_channels:?}",
    );

    // Manufacture a RecvConnection directly via the crate-internal
    // `new_connection_pair` factory, mirroring what
    // `WorldServer::finalize_connection` does after handshake.
    let address: SocketAddr = "127.0.0.1:54300".parse().unwrap();
    let user_key = UserKey::from_u64(42);
    let gwm_guard = recv.state.shared.global_world_manager.read();
    let (recv_conn, _send_conn) = crate::connection::connection::new_connection_pair(
        &recv.state.shared.server_config.connection,
        &recv.state.shared.server_config.ping,
        &address,
        &user_key,
        &recv.state.shared.channel_kinds,
        &*gwm_guard,
        recv.state.shared.server_config.max_replicated_entities as usize,
    );
    drop(gwm_guard);
    recv.state.recv_user_connections.insert(address, recv_conn);

    // Inject a TickBufferedChannel/TestTick message into the connection's
    // tick buffer at host_tick=10, message_tick=10.
    // Per `TickBufferReceiverChannel::insert`, the message_tick must be
    // strictly in the future of host_tick (so the tick-buffer keeps a
    // monotone-future ordering). Pick host=10, message=12; then drain at
    // the message tick.
    let channel_kind = ChannelKind::of::<TestTickBufferedChannel>();
    let host_tick: naia_shared::Tick = 10;
    let message_tick: naia_shared::Tick = 12;
    let payload: Box<dyn Message> = Box::new(TestTick { value: 7 });
    let container = MessageContainer::new(payload);

    let recv_conn = recv
        .state
        .recv_user_connections
        .get_mut(&address)
        .expect("connection just inserted");
    let injected = recv_conn.inject_tick_buffer_message(
        &channel_kind,
        &host_tick,
        &message_tick,
        container,
    );
    assert!(injected, "tick-buffer inject_message should accept the message");

    // Drain via the helper under test.
    let mut messages = drain_tick_buffer(&mut recv, message_tick);
    let decoded: Vec<(UserKey, TestTick)> =
        messages.read::<TestTickBufferedChannel, TestTick>();
    assert_eq!(decoded.len(), 1, "expected exactly one decoded message");
    assert_eq!(decoded[0].0, user_key);
    assert_eq!(decoded[0].1.value, 7);

    // A second drain at the same tick should be empty — receive_messages
    // consumes the buffer.
    let mut messages_again = drain_tick_buffer(&mut recv, message_tick);
    let decoded_again: Vec<(UserKey, TestTick)> =
        messages_again.read::<TestTickBufferedChannel, TestTick>();
    assert!(decoded_again.is_empty(), "tick buffer should drain to empty");
}
