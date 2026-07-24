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

#[allow(unused_imports)]
use naia_shared::DisconnectReason;
use naia_shared::{BigMapKey, GlobalEntitySpawner, Protocol};

use crate::events::world_events::WorldEvents;
use crate::server::receive_output::ReceiveOutput;
use crate::user::{UserKey, WorldUser};
use crate::{NaiaServerError, RecvHandle, SendHandle, ServerConfig};

use super::{
    drain_lifecycle, drain_tick_buffer, spawn_server_handles, CoordHandle, PipelinedWorldServer,
    RecvLifecycleEvent,
};

/// Compile-time assertion that `T: Send`.
fn assert_send<T: Send>() {}

#[test]
fn spawn_server_handles_constructs_three_handles() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (sim_handle, recv, send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // The three handles construct and own their own state. The Arc on
    // CoordHandle::shared is the same allocation as on the recv/send
    // handles, so the strong count must be at least 3.
    let strong = std::sync::Arc::strong_count(&sim_handle.shared);
    assert!(
        strong >= 3,
        "expected at least 3 strong refs to ServerShared after split, got {strong}",
    );

    drop((sim_handle, recv, send));
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

#[test]
fn send_connection_readiness_is_pure_and_tracks_materialization() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();
    let (mut sim, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    let address: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    let user_key = UserKey::from_u64(7);
    sim.receive_user(user_key, address);

    assert!(!crate::server::world_server::user_connection_ready_impl(
        &sim.state.user_store,
        &send.state.send_user_connections,
        &user_key,
    ));

    let gwm = send.state.shared.global_world_manager.read();
    let (_recv_conn, send_conn) = crate::connection::connection::new_connection_pair(
        &send.state.shared.server_config.connection,
        &send.state.shared.server_config.ping,
        &address,
        &user_key,
        &send.state.shared.channel_kinds,
        &*gwm,
        send.state.shared.server_config.max_replicated_entities as usize,
    );
    drop(gwm);
    send.state.send_user_connections.insert(address, send_conn);

    // No bandwidth monitor was enabled: this query must remain a pure
    // park-window membership read and must not panic.
    assert!(crate::server::world_server::user_connection_ready_impl(
        &sim.state.user_store,
        &send.state.send_user_connections,
        &user_key,
    ));
}

mod outbound_message_test_protocol {
    use naia_shared::{Channel, Message};

    #[derive(Channel)]
    pub struct TestServerChannel;

    #[derive(Message, PartialEq, Eq, Hash)]
    pub struct TestServerMessage {
        pub value: u32,
    }
}

#[test]
fn pipelined_send_message_fails_before_materialization_and_after_disconnect() {
    use naia_shared::{ChannelDirection, ChannelMode, ReliableSettings};

    use outbound_message_test_protocol::{TestServerChannel, TestServerMessage};

    let mut proto = Protocol::builder();
    proto
        .add_channel::<TestServerChannel>(
            ChannelDirection::ServerToClient,
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
        )
        .add_message::<TestServerMessage>();
    proto.lock();
    let protocol = proto.build();

    let mut server = PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);
    let address: SocketAddr = "127.0.0.1:54322".parse().unwrap();
    let user_key = UserKey::from_u64(8);
    let message = TestServerMessage { value: 9 };
    server.receive_user(user_key, address);

    assert!(matches!(
        server.send_message::<TestServerChannel, _>(&user_key, &message),
        Err(NaiaServerError::UserNotFound)
    ));

    let (coord, recv, mut send) = server.take_handles();
    let gwm = send.state.shared.global_world_manager.read();
    let (_recv_conn, send_conn) = crate::connection::connection::new_connection_pair(
        &send.state.shared.server_config.connection,
        &send.state.shared.server_config.ping,
        &address,
        &user_key,
        &send.state.shared.channel_kinds,
        &*gwm,
        send.state.shared.server_config.max_replicated_entities as usize,
    );
    drop(gwm);
    send.state.send_user_connections.insert(address, send_conn);
    server.restore_handles(coord, recv, send);

    assert!(
        server
            .send_message::<TestServerChannel, _>(&user_key, &message)
            .is_ok(),
        "materialized send connection must accept the message"
    );

    let (coord, recv, mut send) = server.take_handles();
    send.state.send_user_connections.remove(&address);
    server.restore_handles(coord, recv, send);

    assert!(matches!(
        server.send_message::<TestServerChannel, _>(&user_key, &message),
        Err(NaiaServerError::UserNotFound)
    ));
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
    assert!(
        events.is_empty(),
        "no lifecycle events expected: {events:?}"
    );
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
        .push_error(NaiaServerError::Wrapped(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "boom",
        ))));

    let events = drain_lifecycle(&mut output);
    assert_eq!(events.len(), 3, "expected 3 lifecycle events: {events:?}");

    match &events[0] {
        RecvLifecycleEvent::Connected { user_key } => assert_eq!(*user_key, user_a),
        other => panic!("expected Connected, got {other:?}"),
    }
    match &events[1] {
        RecvLifecycleEvent::Disconnected {
            user_key,
            address,
            reason,
        } => {
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
    assert!(
        leftover.is_empty(),
        "second drain should be empty: {leftover:?}"
    );
}

#[test]
fn drain_tick_buffer_on_empty_recv_handle_is_empty() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (_sim_handle, mut recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

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
        ChannelDirection, ChannelKind, ChannelMode, Message, MessageContainer, TickBufferSettings,
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

    let (_sim_handle, mut recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Sanity-check that the channel kind made it into the shared
    // channel_kinds table — if this is empty, the protocol-build path is
    // broken (and inject_message below would fall through the `else`
    // arm and return false silently).
    let registered_channels = recv.state.shared.channel_kinds.all_names();
    assert!(
        registered_channels
            .iter()
            .any(|n| n.contains("TestTickBufferedChannel")),
        "channel_kinds should contain TestTickBufferedChannel, saw {registered_channels:?}",
    );

    // Manufacture a RecvConnection directly via the crate-internal
    // `new_connection_pair` factory, mirroring what
    // `InternalWorldServer::finalize_connection` does after handshake.
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
    let injected =
        recv_conn.inject_tick_buffer_message(&channel_kind, &host_tick, &message_tick, container);
    assert!(
        injected,
        "tick-buffer inject_message should accept the message"
    );

    // Drain via the helper under test.
    let mut messages = drain_tick_buffer(&mut recv, message_tick);
    let decoded: Vec<(UserKey, TestTick)> = messages.read::<TestTickBufferedChannel, TestTick>();
    assert_eq!(decoded.len(), 1, "expected exactly one decoded message");
    assert_eq!(decoded[0].0, user_key);
    assert_eq!(decoded[0].1.value, 7);

    // A second drain at the same tick should be empty — receive_messages
    // consumes the buffer.
    let mut messages_again = drain_tick_buffer(&mut recv, message_tick);
    let decoded_again: Vec<(UserKey, TestTick)> =
        messages_again.read::<TestTickBufferedChannel, TestTick>();
    assert!(
        decoded_again.is_empty(),
        "tick buffer should drain to empty"
    );
}

/// Integration test for CoordHandle room-ops deferred-drain path (§ 5.4).
///
/// Verifies that:
/// - CoordHandle::create_room, room_add_user, room_add_entity push to
///   scope_change_queue without draining.
/// - apply_pending_room_changes (as called from SendState::send_all_packets)
///   correctly applies those changes to entity_room_map + scope_checks_cache.
#[test]
fn sim_handle_room_ops_deferred_drain_path() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Register entity 42u64 in global_entity_map so room_add_entity can
    // look up its GlobalEntity.
    let global_entity = {
        let mut entity_map = sim_handle.shared.global_entity_map.write();
        entity_map.spawn(42u64, None)
    };

    // Register a user so room_add_user can subscribe it.
    let user_key = UserKey::from_u64(1);
    let user_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    sim_handle
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    // Coord-side room ops — all push-only, no drain.
    let rk = sim_handle.create_room();
    sim_handle.room_add_user(&rk, &user_key);
    sim_handle.room_add_entity(&rk, &42u64);

    // The queue should have 4 entries: (legacy+room) for add_user, (legacy+room) for add_entity.
    {
        let q = sim_handle.shared.scope_change_queue.lock();
        assert!(
            q.len() >= 2,
            "expected at least 2 entries in scope_change_queue before drain, got {}",
            q.len()
        );
    }

    // Drain via apply_pending_room_changes — same path as send_all_packets.
    send.state
        .apply_pending_room_changes(&sim_handle.shared.scope_change_queue);

    // entity_room_map must have entity 42u64 in room rk.
    let erm_rooms = send.state.entity_room_map.entity_get_rooms(&global_entity);
    assert!(
        erm_rooms.map(|set| set.contains(&rk)).unwrap_or(false),
        "entity_room_map should map global_entity → rk after drain"
    );

    // scope_checks_cache.pending_slice() must have the (rk, user_key, 42u64) tuple
    // (entity was added after user, so on_entity_added_to_room adds (rk, user_key, 42u64)).
    let pending = send.state.scope_checks_cache.pending_slice();
    assert!(
        pending.contains(&(rk, user_key, 42u64)),
        "scope_checks_cache.pending_slice should contain (rk, user_key, 42) after drain; got: {pending:?}"
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — unpublish
/// read-before-write capture (B.2 blocker 1).
///
/// `unpublish_entity` captures `owner_addr` from the ClientPublic owner
/// BEFORE `gwm.entity_unpublish` transitions it to Client. The Coord-only
/// `configure_entity_replication` must snapshot that address into the
/// `ConfigureSendOp::Unpublish` payload at queue-PUSH time so the
/// deferred Send drain can still address the formerly-owning connection
/// even though gwm has already moved on.
///
/// This test verifies the capture is taken from the PRE-transition state:
/// after `configure_entity_replication(private)` the gwm owner is `Client`
/// (transitioned), yet the queued op carries the `ClientPublic` owner's
/// address.
#[test]
fn configure_unpublish_captures_owner_addr_before_transition() {
    use crate::server::configure_replication::ConfigureSendOp;
    use crate::server::scope_change::ScopeChange;
    use crate::world::entity_owner::EntityOwner;
    use crate::ReplicationConfig;
    use naia_shared::Publicity;

    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Register a user with a known address.
    let user_key = UserKey::from_u64(7);
    let user_addr: SocketAddr = "127.0.0.1:9007".parse().unwrap();
    sim_handle
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    // Manufacture a client-owned, PUBLIC entity (ClientPublic) in gwm.
    let world_entity: u64 = 77;
    let global_entity = sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(world_entity, None);
    sim_handle
        .shared
        .global_world_manager
        .write()
        .insert_entity_record(&global_entity, EntityOwner::Client(user_key));
    // entity_publish: Client → ClientPublic + Publicity::Public.
    let published = sim_handle
        .shared
        .global_world_manager
        .write()
        .entity_publish(&global_entity);
    assert!(published, "manufactured entity should publish");
    assert!(matches!(
        sim_handle
            .shared
            .global_world_manager
            .read()
            .entity_owner(&global_entity),
        Some(EntityOwner::ClientPublic(_))
    ));

    // Configure Public → Private (unpublish). The Coord method must
    // capture the owner address NOW, while gwm is ClientPublic.
    sim_handle.configure_entity_replication(&world_entity, ReplicationConfig::private());

    // gwm has transitioned to Client (post-write).
    assert!(
        matches!(
            sim_handle
                .shared
                .global_world_manager
                .read()
                .entity_owner(&global_entity),
            Some(EntityOwner::Client(_))
        ),
        "gwm owner must be Client after unpublish (transitioned immediately)",
    );
    assert_eq!(
        sim_handle
            .shared
            .global_world_manager
            .read()
            .entity_replication_config(&global_entity)
            .unwrap()
            .publicity,
        Publicity::Private,
    );

    // The queued ConfigureReplication op must carry the captured
    // ClientPublic owner address (read BEFORE the transition).
    let q = sim_handle.shared.scope_change_queue.lock();
    let mut found_owner_addr: Option<Option<SocketAddr>> = None;
    for change in q.iter() {
        if let ScopeChange::ConfigureReplication(cap) = change {
            for op in &cap.send_ops {
                if let ConfigureSendOp::Unpublish { owner_addr, .. } = op {
                    found_owner_addr = Some(*owner_addr);
                }
            }
        }
    }
    assert_eq!(
        found_owner_addr,
        Some(Some(user_addr)),
        "deferred Unpublish op must carry the owner address captured \
         from the PRE-transition ClientPublic state",
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — `CoordHandle::receive_user`
///
/// Verifies that `CoordHandle::receive_user(user_key, addr)` inserts the user
/// into `sim_handle.state.user_store` exactly as `InternalWorldServer::receive_user` does,
/// confirmed via the existing `user_exists` query method.
#[test]
fn sim_handle_receive_user_inserts_into_user_store() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    let user_key = UserKey::from_u64(100);
    let user_addr: SocketAddr = "127.0.0.1:11000".parse().unwrap();

    // Before receive_user the user must not exist.
    assert!(
        !sim_handle.user_exists(&user_key),
        "user should not exist before receive_user"
    );

    sim_handle.receive_user(user_key, user_addr);

    // After receive_user the user must be present.
    assert!(
        sim_handle.user_exists(&user_key),
        "user should exist after receive_user"
    );

    // The stored address must match.
    assert_eq!(
        sim_handle.user_address(&user_key),
        Some(user_addr),
        "user_address should match the addr passed to receive_user"
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — `CoordHandle::disconnect_user`
/// idempotency on unknown user.
///
/// Calling `disconnect_user` for a user that was never registered must return
/// silently without panicking and must NOT push to `pending_disconnect_requests`.
#[test]
fn sim_handle_disconnect_user_nonexistent_is_noop() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    let unknown_key = UserKey::from_u64(999);

    // Must not panic.
    sim_handle.disconnect_user(&unknown_key);

    // Queue must remain empty.
    let q = sim_handle.shared.pending_disconnect_requests.lock();
    assert!(
        q.is_empty(),
        "pending_disconnect_requests should stay empty for unknown user, got: {q:?}"
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — `CoordHandle::disconnect_user`
/// queues a request for a known user.
///
/// After `receive_user` then `disconnect_user`, `pending_disconnect_requests`
/// must contain exactly one `(user_key, DisconnectReason::Kicked)` entry.
#[test]
fn sim_handle_disconnect_user_queues_request() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, _send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    let user_key = UserKey::from_u64(200);
    let user_addr: SocketAddr = "127.0.0.1:12000".parse().unwrap();

    sim_handle.receive_user(user_key, user_addr);
    sim_handle.disconnect_user(&user_key);

    let q = sim_handle.shared.pending_disconnect_requests.lock();
    assert_eq!(
        q.len(),
        1,
        "pending_disconnect_requests should have exactly 1 entry after disconnect_user"
    );
    let (queued_key, queued_reason) = &q[0];
    assert_eq!(*queued_key, user_key, "queued user_key must match");
    assert!(
        matches!(queued_reason, DisconnectReason::Kicked),
        "queued reason must be Kicked, got: {queued_reason:?}"
    );

    // Calling disconnect_user a second time must be idempotent —
    // the user_store still contains the user (only the recv drain removes it)
    // so a second call pushes a second entry. Idempotency at the QUEUE push
    // level is not required; `user_queue_disconnect` handles dedup via
    // `manual_disconnect`. But calling disconnect_user on a non-existent key
    // (after the recv drain has removed it) must be a no-op (tested above).
    drop(q);
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.4 (2026-05-19) — `SendHandle::scope_checks_pending`
/// and `SendHandle::mark_scope_checks_pending_handled`.
///
/// Verifies that pending scope-check tuples appear after a room add-user +
/// add-entity sequence (via the CoordHandle room ops → preamble drain path), and
/// that `mark_scope_checks_pending_handled` clears the pending slice.
#[test]
fn send_handle_scope_checks_pending_and_mark_handled() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Register entity 10u64 in global_entity_map.
    sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(10u64, None);

    // Register a user.
    let user_key = UserKey::from_u64(1);
    let user_addr: SocketAddr = "127.0.0.1:20001".parse().unwrap();
    sim_handle
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    // Coord room ops — push-only.
    let rk = sim_handle.create_room();
    sim_handle.room_add_user(&rk, &user_key);
    sim_handle.room_add_entity(&rk, &10u64);

    // Before drain, pending should be empty (scope cache not yet updated).
    let pending_before = send.scope_checks_pending();
    assert!(
        pending_before.is_empty(),
        "pending should be empty before preamble drain, got: {pending_before:?}"
    );

    // Drain via apply_pending_room_changes — same path as send_all_packets preamble.
    send.state
        .apply_pending_room_changes(&sim_handle.shared.scope_change_queue);

    // After drain, pending must contain the (room, user, entity) tuple.
    let pending_after = send.scope_checks_pending();
    assert!(
        pending_after.contains(&(rk, user_key, 10u64)),
        "scope_checks_pending should contain (rk, user_key, 10) after drain, got: {pending_after:?}"
    );

    // Mark handled — pending slice must clear.
    send.mark_scope_checks_pending_handled();
    let pending_cleared = send.scope_checks_pending();
    assert!(
        pending_cleared.is_empty(),
        "scope_checks_pending should be empty after mark_pending_handled, got: {pending_cleared:?}"
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.4 (2026-05-19) — `SendHandle::user_scope_has_entity`
/// with explicit include / exclude.
///
/// Verifies that:
/// - `user_scope_has_entity` returns `true` after `user_scope_set_entity(include)`.
/// - `user_scope_has_entity` returns `false` after `user_scope_set_entity(exclude)`.
/// - `is_resource=false` is exercised (server-owned, in a room → include wins normally).
#[test]
fn send_handle_user_scope_has_entity_explicit_include_exclude() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Register entity 42u64 as a server-owned entity.
    let world_entity: u64 = 42;
    let global_entity = sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(world_entity, None);
    let idx = sim_handle
        .shared
        .global_world_manager
        .write()
        .insert_entity_record(
            &global_entity,
            crate::world::entity_owner::EntityOwner::Server,
        );
    if idx.is_valid() {
        sim_handle.shared.idx_to_world.write()[idx.as_usize()] = Some(world_entity);
    }

    // Register a user.
    let user_key = UserKey::from_u64(2);
    let user_addr: SocketAddr = "127.0.0.1:20002".parse().unwrap();
    sim_handle
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    // Put the entity in a room so it is not roomless (avoids the roomless
    // server-owned gate that would veto explicit include on non-resources).
    let rk = sim_handle.create_room();
    sim_handle.room_add_user(&rk, &user_key);
    sim_handle.room_add_entity(&rk, &world_entity);

    // Drain pending room changes so entity_room_map + user_room_map are current.
    send.state
        .apply_pending_room_changes(&sim_handle.shared.scope_change_queue);

    // Set explicit include.
    let set_ok = send.user_scope_set_entity(&user_key, &world_entity, true);
    assert!(
        set_ok,
        "user_scope_set_entity should return true for a registered entity"
    );

    // user_scope_has_entity must return true (is_resource=false, entity is in a room).
    assert!(
        send.user_scope_has_entity(&user_key, &world_entity, false),
        "user_scope_has_entity should return true after explicit include"
    );

    // Set explicit exclude.
    send.user_scope_set_entity(&user_key, &world_entity, false);
    assert!(
        !send.user_scope_has_entity(&user_key, &world_entity, false),
        "user_scope_has_entity should return false after explicit exclude"
    );
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.3b.4 (2026-05-19) — `SendHandle::user_scope_has_entity`
/// default room-sharing logic.
///
/// Without an explicit scope bit, an entity in the same room as the user
/// must be in-scope; an entity in a different room must not be in-scope.
#[test]
fn send_handle_user_scope_has_entity_room_default() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (mut sim_handle, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    // Register two entities.
    let entity_in_room: u64 = 100;
    let entity_not_in_room: u64 = 200;
    let ge_in = sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(entity_in_room, None);
    let ge_out = sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(entity_not_in_room, None);
    for ge in [ge_in, ge_out] {
        let idx = sim_handle
            .shared
            .global_world_manager
            .write()
            .insert_entity_record(&ge, crate::world::entity_owner::EntityOwner::Server);
        if idx.is_valid() {
            sim_handle.shared.idx_to_world.write()[idx.as_usize()] = Some(entity_in_room);
        }
    }

    // Register a user.
    let user_key = UserKey::from_u64(3);
    let user_addr: SocketAddr = "127.0.0.1:20003".parse().unwrap();
    sim_handle
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    // Only entity_in_room goes into the room with the user.
    let rk = sim_handle.create_room();
    sim_handle.room_add_user(&rk, &user_key);
    sim_handle.room_add_entity(&rk, &entity_in_room);

    // entity_not_in_room goes into a different room the user is NOT in.
    let rk2 = sim_handle.create_room();
    sim_handle.room_add_entity(&rk2, &entity_not_in_room);

    // Drain pending room changes.
    send.state
        .apply_pending_room_changes(&sim_handle.shared.scope_change_queue);

    // entity_in_room: in same room as user → in-scope by default.
    assert!(
        send.user_scope_has_entity(&user_key, &entity_in_room, false),
        "entity sharing a room with the user should be in-scope by default"
    );

    // entity_not_in_room: in a different room → not in-scope by default.
    assert!(
        !send.user_scope_has_entity(&user_key, &entity_not_in_room, false),
        "entity in a different room should NOT be in-scope by default"
    );
}

// ── task #13: pipelined priority publish (coord → send) ──────────────────────
//
// Drives the REAL `publish_priority` (the split-engine analog of resident
// `run_send_preamble`'s `clone_from`) and inspects the live `send` state via the
// park-window slot. Proves coord-side priority writes reach `send` — the gap
// task #13 closes — and that the per-user merge preserves accumulators across
// publishes, handles `reset()` across ticks, and clears the staging each tick.

#[test]
fn pipelined_priority_publish_global_and_per_user() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();
    let mut server = crate::PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);

    let uk = UserKey::from_u64(7);
    let e: u64 = 42;

    // Coord-side writes (global mirror + per-user staging) — not yet in `send`.
    server.global_entity_priority_mut(e).set_gain(3.0);
    server
        .user_entity_priority_mut(&uk, e)
        .set_gain(5.0)
        .boost_once(10.0);

    server.publish_priority_for_test();

    {
        let slot = server.send_slot();
        let lock = slot.lock();
        let send = lock.as_ref().unwrap();
        assert_eq!(send.state.global_priority.gain_override(&e), Some(3.0));
        let layer = send
            .state
            .user_priorities
            .get(&uk)
            .expect("per-user layer published into send");
        assert_eq!(layer.gain_override(&e), Some(5.0));
        assert_eq!(layer.accumulated(&e), 10.0);
    }
    // Staging is drained + cleared.
    assert!(
        server
            .coord()
            .state
            .user_priority_staging
            .get(&uk)
            .map_or(true, |l| l.is_empty()),
        "per-user staging must be cleared after publish",
    );

    // A second publish with NO new coord writes: global gain re-clones
    // (idempotent), per-user gain persists send-side, accumulator is NOT
    // re-boosted (no double-application).
    server.publish_priority_for_test();
    {
        let slot = server.send_slot();
        let lock = slot.lock();
        let send = lock.as_ref().unwrap();
        assert_eq!(send.state.global_priority.gain_override(&e), Some(3.0));
        let layer = send.state.user_priorities.get(&uk).unwrap();
        assert_eq!(
            layer.gain_override(&e),
            Some(5.0),
            "per-user gain persists send-side"
        );
        assert_eq!(
            layer.accumulated(&e),
            10.0,
            "no double-boost from a no-op publish"
        );
    }

    // `reset()` in a LATER tick (staging already cleared) must still reach the
    // persisted send gain — the case a state-based mirror cannot express and the
    // `gain_dirty` flag exists for.
    server.user_entity_priority_mut(&uk, e).reset();
    server.publish_priority_for_test();
    {
        let slot = server.send_slot();
        let lock = slot.lock();
        let send = lock.as_ref().unwrap();
        let layer = send.state.user_priorities.get(&uk).unwrap();
        assert_eq!(
            layer.gain_override(&e),
            None,
            "reset reached send across ticks"
        );
        assert_eq!(
            layer.accumulated(&e),
            10.0,
            "reset must not touch the accumulator"
        );
    }
}
