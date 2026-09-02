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
        &gwm,
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
        &gwm,
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
        .push_error(NaiaServerError::Wrapped(Box::new(std::io::Error::other(
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
        &gwm_guard,
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
            .is_none_or(|l| l.is_empty()),
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

/// TrueSight L6 (§15.5) — the pipelined half of the one-shot scope-exit
/// override.
///
/// The behavioural suite runs against the Resident engine
/// (`test/harness/contract_tests/integration_only/06_entity_scopes.rs`); both
/// engines consult the same `EntityScopeMap` at the same exit sites, so what
/// the pipelined shape has to prove is its own wiring: the send-side writer
/// reaches the ledger, and the `&mut` API stages a D7 op rather than dropping
/// the arming on the floor.
#[test]
fn pipelined_arming_reaches_the_send_side_scope_ledger() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let (sim_handle, _recv, mut send) =
        spawn_server_handles::<u64, _>(ServerConfig::default(), protocol).take_handles();

    let world_entity: u64 = 42;
    let global_entity = sim_handle
        .shared
        .global_entity_map
        .write()
        .spawn(world_entity, None);
    let user_key = UserKey::from_u64(2);
    let other_user = UserKey::from_u64(3);

    // Send-side writer: arms exactly the pair it names.
    send.user_scope_despawn_on_next_exit_global(&user_key, global_entity);
    assert!(
        send.state
            .entity_scope_map
            .has_despawn_on_next_exit(&user_key, &global_entity),
        "the send-side writer should arm the named pair"
    );
    assert!(
        !send
            .state
            .entity_scope_map
            .has_despawn_on_next_exit(&other_user, &global_entity),
        "arming one user must not arm another"
    );

    // Arming is idempotent, and firing consumes it exactly once.
    send.user_scope_despawn_on_next_exit_global(&user_key, global_entity);
    assert!(
        send.state
            .entity_scope_map
            .take_despawn_on_next_exit(&user_key, &global_entity),
        "the first exit after arming should fire"
    );
    assert!(
        !send
            .state
            .entity_scope_map
            .take_despawn_on_next_exit(&user_key, &global_entity),
        "a second exit must not fire again"
    );

    // Re-entry disarms without firing.
    send.user_scope_despawn_on_next_exit_global(&user_key, global_entity);
    send.state
        .entity_scope_map
        .clear_despawn_on_next_exit(&user_key, &global_entity);
    assert!(
        !send
            .state
            .entity_scope_map
            .take_despawn_on_next_exit(&user_key, &global_entity),
        "re-entry should have disarmed the override"
    );
}

/// TrueSight L6 (§15.5) — the `&mut` API stages a D7 scope-ledger op.
#[test]
fn pipelined_despawn_on_next_exit_stages_a_scope_ledger_op() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let mut server = crate::PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);

    let world_entity: u64 = 42;
    let user_key = UserKey::from_u64(2);

    assert!(server.coord().state.pending_scope_ledger_ops.is_empty());
    server.user_scope_despawn_on_next_exit(&user_key, &world_entity);

    let staged = &server.coord().state.pending_scope_ledger_ops;
    assert_eq!(staged.len(), 1, "arming should stage exactly one ledger op");
    match &staged[0] {
        crate::server::coord_state::PendingScopeLedgerOp::DespawnOnNextExit {
            user_key: staged_user,
            world_entity: staged_entity,
        } => {
            assert_eq!(staged_user, &user_key);
            assert_eq!(staged_entity, &world_entity);
        }
        _ => panic!("arming staged the wrong scope-ledger op"),
    }
}

/// Read every tracked `(user, global entity)` scope entry out of the parked
/// send handle, in a fixed order, so two snapshots compare entry-for-entry.
fn scope_ledger_snapshot(
    server: &crate::PipelinedWorldServer<u64>,
    pairs: &[(UserKey, naia_shared::GlobalEntity)],
) -> Vec<Option<bool>> {
    let slot = server.send_slot();
    let lock = slot.lock();
    let send = lock.as_ref().expect("send handle must be parked");
    pairs
        .iter()
        .map(|(user_key, global_entity)| {
            send.state
                .entity_scope_map
                .get(user_key, global_entity)
                .copied()
        })
        .collect()
}

/// Deferred-scope stale-entity contract.
///
/// A deferred `PendingScopeLedgerOp::Set` stores the RAW world entity and is
/// drained a window after it was queued, so the entity may have terminally
/// despawned in between — its `GlobalEntityMap` association removed by
/// `despawn_by_world` / `despawn_by_global`. There is then nothing to include
/// or exclude (the client-side removal already travels on the entity-despawn
/// op), so resolving it must be a no-op that touches no user's scope ledger —
/// exactly as the immediate `SendHandle::user_scope_set_entity` and the
/// deferred `DespawnOnNextExit` arm already behave. Before this fix the `Set`
/// arm `.unwrap()`ed the lookup and aborted the process.
///
/// Mutant guard: restoring the `.unwrap()` panics in the stale phase below, so
/// the smallest mutant dies. The live-mapping control keeps that non-vacuous by
/// proving both `is_contained` values still mutate the intended pair.
#[test]
fn deferred_scope_set_tolerates_a_stale_entity_without_touching_any_ledger() {
    use crate::server::coord_state::PendingScopeLedgerOp;
    use naia_shared::EntityAndGlobalEntityConverter;

    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();

    let mut server = crate::PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);

    let live_entity: u64 = 7;
    let stale_entity: u64 = 42;
    let bystander_entity: u64 = 9;

    let victim = UserKey::from_u64(2);
    let other = UserKey::from_u64(3);

    let (live_ge, stale_ge, bystander_ge) = {
        let mut map = server.coord().shared.global_entity_map.write();
        (
            map.spawn(live_entity, None),
            map.spawn(stale_entity, None),
            map.spawn(bystander_entity, None),
        )
    };

    // Every pair the assertions below track, in a fixed order.
    let tracked = [
        (victim, live_ge),
        (victim, stale_ge),
        (victim, bystander_ge),
        (other, live_ge),
        (other, stale_ge),
        (other, bystander_ge),
    ];

    // ── Live-mapping control: both `is_contained` values mutate the named pair
    //    and nothing else. This is what makes the stale-phase assertions
    //    non-vacuous — it proves the drain really does write when it can.
    for is_contained in [true, false] {
        server
            .coord_mut()
            .state
            .pending_scope_ledger_ops
            .push(PendingScopeLedgerOp::Set {
                user_key: victim,
                world_entity: live_entity,
                is_contained,
            });
        server.drain_pending_scope_ledger_ops_for_test();
        let snap = scope_ledger_snapshot(&server, &tracked);
        assert_eq!(
            snap[0],
            Some(is_contained),
            "a live-mapping Set({is_contained}) must write the named pair",
        );
        assert_eq!(
            &snap[1..],
            &[None, None, None, None, None][..],
            "a live-mapping Set must not touch any other user or entity",
        );
    }

    // ── The mapping exists right up until the entity terminally despawns.
    assert!(
        server
            .coord()
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(&stale_entity)
            .is_ok(),
        "precondition: the entity is mapped before it despawns",
    );

    let before = scope_ledger_snapshot(&server, &tracked);
    let scope_changes_before = server.coord().shared.scope_change_queue.lock().len();

    // ── Stage both `is_contained` values, THEN terminally despawn the entity —
    //    the exact queue-then-retire ordering the projectile race produces.
    for is_contained in [false, true] {
        server
            .coord_mut()
            .state
            .pending_scope_ledger_ops
            .push(PendingScopeLedgerOp::Set {
                user_key: victim,
                world_entity: stale_entity,
                is_contained,
            });
    }
    {
        server
            .coord()
            .shared
            .global_entity_map
            .write()
            .despawn_by_world(&stale_entity);
    }
    assert!(
        server
            .coord()
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(&stale_entity)
            .is_err(),
        "precondition: the mapping is absent once the entity has despawned",
    );

    // Must not panic.
    server.drain_pending_scope_ledger_ops_for_test();

    // ── Every tracked entry is identical to before the stale drain, including
    //    the victim's own entry for the stale entity (which stays absent).
    let after = scope_ledger_snapshot(&server, &tracked);
    assert_eq!(
        after, before,
        "a stale Set must leave every user's scope ledger entry-identical",
    );
    assert_eq!(
        after[1], None,
        "a stale Set must not create an entry for the despawned entity",
    );
    assert_eq!(
        server.coord().shared.scope_change_queue.lock().len(),
        scope_changes_before,
        "a stale Set must not enqueue a ScopeToggled change",
    );
    assert!(
        server.coord().state.pending_scope_ledger_ops.is_empty(),
        "the drain consumes the staged ops regardless of staleness",
    );
}

/// Split-engine room-removal queue lifecycle. `Room::remove_entity` and
/// `Room::unsubscribe_user` queue one `(user, entity)` per member for the
/// fused Loop 1 drain, which the split engine never runs. Prove the queue
/// does fill under coord-side churn (the control), and that one `send`
/// tick empties it.
#[test]
fn pipelined_room_churn_cannot_grow_the_removal_queue() {
    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();
    let mut server = crate::PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);

    let entity: u64 = 42;
    {
        let coord = server.coord_mut();
        let mut entity_map = coord.shared.global_entity_map.write();
        entity_map.spawn(entity, None);
    }
    let user_key = UserKey::from_u64(1);
    let user_addr: SocketAddr = "127.0.0.1:9013".parse().unwrap();
    server
        .coord_mut()
        .state
        .user_store
        .insert(user_key, WorldUser::new(user_addr));

    let rk = server.coord_mut().create_room();
    server.coord_mut().room_add_user(&rk, &user_key);

    // Control: entity churn queues one entry per removal ...
    for _ in 0..50 {
        server.coord_mut().room_add_entity(&rk, &entity);
        server.coord_mut().room_remove_entity(&rk, &entity);
    }
    // ... and user churn queues one entry per entity in the room per leave.
    server.coord_mut().room_add_entity(&rk, &entity);
    for _ in 0..50 {
        server.coord_mut().room_remove_user(&rk, &user_key);
        server.coord_mut().room_add_user(&rk, &user_key);
    }
    assert_eq!(
        server
            .coord()
            .state
            .room_store
            .entity_removal_queue_len(&rk),
        100,
        "control: coord-side room churn must queue Loop 1 removals"
    );

    // One split-engine tick drops them all.
    let world = naia_shared::SnapshotWorld::<u64>::new();
    server.send(&world);
    assert_eq!(
        server
            .coord()
            .state
            .room_store
            .entity_removal_queue_len(&rk),
        0,
        "drain_and_send must discard the removal queue"
    );

    // Steady state: churn between ticks never accumulates across ticks.
    for _ in 0..3 {
        server.coord_mut().room_remove_entity(&rk, &entity);
        server.coord_mut().room_add_entity(&rk, &entity);
        server.send(&world);
        assert_eq!(
            server
                .coord()
                .state
                .room_store
                .entity_removal_queue_len(&rk),
            0
        );
    }
}

/// Feature-free in-process socket for the split-engine falsifier: packets go
/// into an unbounded `PacketChannel` nobody drains, auth is a no-op.
struct SinkSocket;

impl From<SinkSocket> for Box<dyn crate::transport::Socket> {
    fn from(s: SinkSocket) -> Self {
        Box::new(s)
    }
}

impl crate::transport::Socket for SinkSocket {
    fn listen(self: Box<Self>) -> crate::transport::ListenResult {
        let (ps, pr) = crate::transport::PacketChannel::unbounded();
        (Box::new(SinkAuth), Box::new(SinkAuth), ps, pr)
    }
}

#[derive(Clone)]
struct SinkAuth;

impl crate::transport::AuthSender for SinkAuth {
    fn accept(
        &self,
        _address: &SocketAddr,
        _identity_token: &naia_shared::IdentityToken,
    ) -> Result<(), crate::transport::SendError> {
        Ok(())
    }
    fn reject(
        &self,
        _address: &SocketAddr,
        _payload: Option<&[u8]>,
    ) -> Result<(), crate::transport::SendError> {
        Ok(())
    }
}

impl crate::transport::AuthReceiver for SinkAuth {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, crate::transport::RecvError> {
        Ok(None)
    }
}

/// Split-engine twin of `[entity-scopes-14]`: a user joining a room that
/// already holds several entities gets them spawned in the same order on
/// every fresh `PipelinedWorldServer` in one process. Host entity ids are
/// issued in spawn-command order, so they are the observable.
fn split_room_join_spawn_order() -> Vec<u64> {
    use naia_shared::{EntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverter};

    let mut proto = Protocol::builder();
    proto.lock();
    let protocol = proto.build();
    let mut server = crate::PipelinedWorldServer::<u64>::new(ServerConfig::default(), protocol);
    server.listen(SinkSocket);

    let user_key = UserKey::from_u64(1);
    let user_addr: SocketAddr = "127.0.0.1:9021".parse().unwrap();
    server.receive_user(user_key, user_addr);
    {
        let slot = server.send_slot();
        let mut guard = slot.lock();
        let send = guard
            .as_mut()
            .expect("send handle is in its slot between ticks");
        let gwm = send.state.shared.global_world_manager.read();
        let (_recv_conn, send_conn) = crate::connection::connection::new_connection_pair(
            &send.state.shared.server_config.connection,
            &send.state.shared.server_config.ping,
            &user_addr,
            &user_key,
            &send.state.shared.channel_kinds,
            &gwm,
            send.state.shared.server_config.max_replicated_entities as usize,
        );
        drop(gwm);
        send.state
            .send_user_connections
            .insert(user_addr, send_conn);
    }

    let room = server.create_room();
    let entities: Vec<u64> = (100..108).collect();
    let mut world = naia_shared::SnapshotWorld::<u64>::new();
    for entity in &entities {
        server.enable_entity_replication(entity);
        server.room_add_entity(&room, entity);
        world.mark_live(*entity);
    }
    // Settle the entity entries on their own tick so the join below is the
    // only thing that puts them in the user's scope.
    server.send(&world);
    server.room_add_user(&room, &user_key);
    server.send(&world);

    let global_entities: Vec<_> = {
        let map = server.coord().shared.global_entity_map.read();
        entities
            .iter()
            .map(|e| map.entity_to_global_entity(e).expect("spawned above"))
            .collect()
    };
    let slot = server.send_slot();
    let guard = slot.lock();
    let send = guard
        .as_ref()
        .expect("send handle is in its slot between ticks");
    let send_conn = send
        .state
        .send_user_connections
        .get(&user_addr)
        .expect("send connection registered above");
    let converter = send_conn.base.world_manager.entity_converter();
    let mut by_host_id: Vec<(u32, u64)> = entities
        .iter()
        .zip(global_entities.iter())
        .map(|(entity, global_entity)| {
            let host = converter
                .global_entity_to_host_entity(global_entity)
                .expect("room join must spawn every room entity for the user");
            (host.value(), *entity)
        })
        .collect();
    by_host_id.sort();
    by_host_id.into_iter().map(|(_, entity)| entity).collect()
}

#[test]
fn pipelined_room_join_spawns_entities_in_the_same_order_on_every_fresh_server() {
    let first = split_room_join_spawn_order();
    let second = split_room_join_spawn_order();
    assert_eq!(
        first, second,
        "entity-scopes-14 (split engine): room-join spawn order must not depend on hash state"
    );
}
