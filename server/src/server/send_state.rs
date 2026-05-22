//! Send-thread state owned by the pipeline coordinator (step 4-E).
//!
//! After `WorldServer::into_pipeline_states()` consumes the WorldServer,
//! `SendState<E>` carries every field the send thread needs:
//! `send_user_connections` (the send halves of every connection), the
//! per-user priority layer, the outbound `PacketSender`, and a clone of
//! `Arc<ServerShared<E>>` for lock-free access to init-only config and
//! per-connection cross-thread atomics.
//!
//! `SendHandle<E>` owns a `SendState<E>` directly (not via `Arc<Mutex>`)
//! so the send thread runs without contending with the recv or
//! coordinator threads for connection state.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::{atomic::Ordering, Arc},
};

// HashSet/HashMap imported above are used in the new mirror fields for C.6 prep #6.

use parking_lot::Mutex;

use log::warn;

use naia_shared::{
    BitWriter, Channel, ChannelKind, ComponentKind, EntityAndGlobalEntityConverter,
    EntityAuthStatus, FrozenGlobalDirty, GlobalEntity, GlobalEntityIndex, GlobalEntityMap,
    GlobalPriorityState, GlobalEntitySpawner, GlobalWorldManagerType, HostType, Instant, Message,
    MessageContainer, OutgoingPacket, OutgoingPriorityHook, OwnedBitReader, PacketType, Replicate,
    Serde, SnapshotMap, Tick, Timer, UserPriorityState, WorldMutType, WorldRefType,
};

use crate::{
    connection::{io::SendIo, RecvConnection, SendConnection},
    room::RoomKey,
    server::{
        coord_state::CoordinatorState,
        scope_change::{RoomChange, ScopeChange},
        scope_checks_cache::ScopeChecksCache,
        ServerShared,
    },
    time_manager::TimeManager,
    user::UserKey,
    world::{
        entity_owner::EntityOwner, entity_room_map::EntityRoomMap,
        entity_scope_map::EntityScopeMap, global_world_manager::GlobalWorldManager,
    },
    ScopeExit,
};

/// Send-thread-exclusive state lifted out of `WorldServer` (step 4-E).
pub struct SendState<E: Copy + Eq + Hash + Send + Sync> {
    /// Per-address map of send-side connection halves. Each holds a clone
    /// of the matching connection's `Arc<ConnectionShared>` for ack/RTT
    /// reads.
    pub send_user_connections: HashMap<SocketAddr, SendConnection>,

    /// Per-user priority layer. Entries evicted on scope exit; whole
    /// map entry dropped on disconnect (handled by the coordinator).
    pub user_priorities: HashMap<UserKey, UserPriorityState<E>>,

    /// Sender-wide priority layer (step 4-E.2e). Authoritative for the
    /// Iris send-path read. Kept in sync with `sim_handle.global_priority_mirror`
    /// via publish-on-read at the top of `send_all_packets` — the borrow
    /// API `global_entity_priority_mut` would need a public-API change in
    /// `naia-shared::EntityPriorityMut` to push per-entity updates through
    /// the `SendStateUpdate` queue. A later commit can rewire that path
    /// (the `SendStateUpdate::PriorityChanged` variant is already defined).
    pub global_priority: GlobalPriorityState<E>,

    /// Periodic heartbeat send cadence (relocated from `RecvState` in
    /// 4-F.naia.c.2a). Fires when `handle_heartbeats` should sweep every
    /// `send_user_connection` for an outbound heartbeat packet — entirely
    /// send-side state, so it lives here now that the send half drives
    /// the dispatch loop.
    pub(crate) heartbeat_timer: Timer,

    /// Periodic ping send cadence (relocated from `RecvState` in
    /// 4-F.naia.c.2c). Send-side because the dispatch (`write_header` +
    /// `send_io.send_packet` + `mark_sent`) is send-side; only the
    /// per-user `ping_manager.write_ping(...)` read crosses into the
    /// recv half, and the coordination/serial caller passes a `&mut recv_conns`
    /// borrow for that.
    pub(crate) ping_timer: Timer,

    /// Send half of the transport (step 4-E.2a). Carries the encoder,
    /// outgoing bandwidth monitor, and per-tick byte counter alongside
    /// the `Box<dyn PacketSender>`. Owned here so the send thread has
    /// exclusive mutable access without locking.
    pub send_io: SendIo,

    /// Shared init-only config + cross-thread atomic cells.
    pub shared: Arc<ServerShared<E>>,

    /// Entity ↔ room membership index (relocated from `CoordinatorState`
    /// in Phase A.3 of MISSION_SIM_OWNS_WORLD). Read by Iris during
    /// `send_all_packets` (room-gate decisions) and mutated when entities
    /// enter / leave rooms. Send-side because the dispatching code that
    /// reads it (Iris + per-user scope-policy hook) is already send-side.
    pub(crate) entity_room_map: EntityRoomMap,

    /// Entity ↔ per-user scope membership index (relocated from
    /// `CoordinatorState` in Phase A.3). Mutated by
    /// `user_scope_set_entity` (typically called by D5's send-side scope
    /// policy in cyberlith). Read by `user_scope_has_entity`.
    pub(crate) entity_scope_map: EntityScopeMap,

    /// Push-based mirror of pending scope checks (relocated from
    /// `CoordinatorState` in Phase A.3). Send-side because the scope
    /// policy that drains it (`scope_checks_pending` /
    /// `mark_scope_checks_pending_handled`) is itself send-side under D5.
    pub(crate) scope_checks_cache: ScopeChecksCache<E>,

    /// C.6 prep — `apply_pending_send_preamble` was called this tick
    /// (the new public split point on `SendHandle`). When `true`,
    /// `send_all_packets` skips its built-in preamble call and resets
    /// the flag so the next tick starts from `false`. When `false`
    /// (existing callers that go straight to `send_all_packets`),
    /// `send_all_packets` runs the preamble inline as before — fully
    /// backward compatible.
    pub(crate) preamble_done_this_tick: bool,

    /// C.6 prep #6 — `apply_pending_scope_changes` was called this tick
    /// (the entity-scope fanout sibling of the preamble split). When
    /// `true`, `send_all_packets` skips its auto-call and resets the
    /// flag. Same pattern as `preamble_done_this_tick`.
    pub(crate) scope_changes_done_this_tick: bool,

    /// C.6 prep #6 — Send-side mirror of `User.room_keys()` (which lives
    /// on sim_handle's `UserStore`). Maintained by `apply_pending_room_changes`
    /// via the `RoomChange::UserAdded`/`UserRemoved`/`RoomDestroyed` data.
    /// Read by `apply_pending_scope_changes` when evaluating
    /// `ScopeToggled` (room-intersection logic) and `UserLeftRoom`.
    pub(crate) user_room_map: HashMap<UserKey, HashSet<RoomKey>>,

    /// C.6 prep #6 — Send-side mirror of `Room::user_keys()`. Used by
    /// `apply_pending_scope_changes` when handling `UserEnteredRoom`
    /// (need all users in the room to fan out a single entity's spawn
    /// intent). Maintained alongside `room_entities_map` in
    /// `apply_pending_room_changes`.
    pub(crate) room_users_map: HashMap<RoomKey, HashSet<UserKey>>,

    /// C.6 prep #6 — Send-side mirror of `Room::entities()` paired with
    /// the matching world entity. Used by `apply_pending_scope_changes`
    /// when handling `UserEnteredRoom` (need all entities-in-room to
    /// fan out a single user's scope evaluation).
    pub(crate) room_entities_map: HashMap<RoomKey, HashMap<GlobalEntity, E>>,

    /// Phase A of MISSION_USER_ONLY_SEES_SIM (2026-05-19) — per-(user,
    /// entity) retry counter for `ScopeToggled` re-queues issued by
    /// `apply_scope_for_user` when `world.has_entity` returns false.
    ///
    /// Background: the 2026-05-19 f3-saga finding during
    /// MISSION_SIM_OWNS_WORLD showed that an unbounded re-queue can
    /// retry indefinitely (memory growth) while the prior implementation
    /// effectively gave up after ~2 ticks (lost scope events on any
    /// pipeline-lag). Bounded retry (N=`SCOPE_RETRY_MAX`) restores
    /// survivability without unbounded growth.
    ///
    /// Send-thread-exclusive (no Mutex): the counter is read/written
    /// only inside `apply_scope_for_user`, which is called from
    /// `apply_pending_scope_changes` (also send-side).
    pub(crate) scope_retry_counts: HashMap<(UserKey, GlobalEntity), u8>,
}

/// Phase A of MISSION_USER_ONLY_SEES_SIM (2026-05-19) — maximum number
/// of retries an entry in the `scope_change_queue` may accumulate when
/// the target world entity is not (yet) present in the snapshot world
/// passed to `apply_pending_scope_changes`.
///
/// At the canonical 25 Hz tick rate, N=16 retries spans ~640ms of
/// pipeline lag, which comfortably absorbs any plausible cyberlith
/// Sim → Send → snapshot-build delay while bounding memory.
///
/// On exhaustion the entry is dropped and `warn!` logged. On
/// successful publish (`world.has_entity == true` proceeds past the
/// guard) the counter is reset.
pub(crate) const SCOPE_RETRY_MAX: u8 = 16;

impl<E: Copy + Eq + Hash + Send + Sync> SendState<E> {
    /// Periodic ping dispatch (step 4-F.naia.c.2c — relocated from
    /// `WorldServer::handle_pings`).
    ///
    /// Send-side because every operation other than
    /// `recv_conn.ping_manager.write_ping(...)` is send-side: the
    /// `ping_timer` lives on `SendState`, the per-user header is
    /// assembled via `send_conn.write_header` (reads the cross-half ack
    /// snapshot atomic), the packet is dispatched via `self.send_io`,
    /// and `mark_sent` updates the send-side heartbeat suppression. The
    /// `&mut recv_conns` borrow exists solely so each user's
    /// `ping_manager.should_send_ping()` / `write_ping(...)` can run
    /// against the recv-half body data.
    ///
    /// Serial mode: called from `WorldServer::receive_all_packets` with
    /// `&mut self.recv.recv_user_connections`. Pipeline mode: called by
    /// the coordinator with `&mut recv_handle.state.recv_user_connections`
    /// either standalone or alongside `process_recv_packets`.
    pub fn send_pings(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
    ) {
        if !self.ping_timer.ringing() {
            return;
        }
        self.ping_timer.reset();

        let tm_guard = self.shared.time_manager.read();
        let tm = &*tm_guard;
        for (user_address, recv_conn) in recv_conns.iter_mut() {
            if !recv_conn.ping_manager.should_send_ping() {
                continue;
            }
            let Some(send_conn) = self.send_user_connections.get_mut(user_address) else {
                continue;
            };
            let mut writer = BitWriter::new();

            // write header (send side, reads ack snapshot atomic)
            let _header = send_conn.write_header(PacketType::Ping, &mut writer);

            // write server tick
            tm.current_tick().ser(&mut writer);
            tm.current_tick_instant().ser(&mut writer);

            // write body (recv side)
            recv_conn.ping_manager.write_ping(&mut writer, tm);

            if self
                .send_io
                .send_packet(user_address, writer.to_packet())
                .is_err()
            {
                // Ping send failure is not fatal: the connection timeout
                // will detect a persistently dead link via missed pongs.
                warn!("Server Error: Cannot send ping packet to {}", user_address);
            }
            send_conn.base.mark_sent();
        }
    }

    /// Coordinator-stage cross-half processing (step 4-F.naia.c.2b).
    ///
    /// Runs the work that used to live at the tail of
    /// `WorldServer::receive_all_packets` and inside
    /// `decode_pending_data_packets`. Takes a `&mut` borrow of the
    /// recv-side connection map so the tick-buffer decode can mutate
    /// `RecvConnection::tick_buffer` alongside the send-side ack /
    /// world-manager updates without breaking the recv/send state
    /// separation: in pipeline mode the coordinator can hand both halves
    /// in concurrently because both threads are paused on the coordination-stage
    /// handoff.
    ///
    /// Sequence (in order):
    /// 1. Per-address `send_conn.drain_acks(&mut [])` over every
    ///    `received_addresses` entry — replaces the per-packet drains
    ///    that used to fire inline in the Heartbeat / Ping / Pong recv
    ///    handlers. The `AckSample` channel is crossbeam-backed and FIFO,
    ///    so coalescing per-packet drains into one per-address drain
    ///    preserves delivery-notification order within each connection
    ///    (see 4-F.naia.c.1 as-landed lesson #6 — the broken draft
    ///    consolidated *across* addresses; this method does not).
    /// 2. Per-data-packet tick-buffer decode + message/world decode +
    ///    arm-empty-ack. Drain is intentionally NOT repeated here — step
    ///    1 already drained every address that sent at least one packet
    ///    (Data-only addresses are a subset of `received_addresses`).
    /// 3. Per-address `send_conn.process_received_commands()` over every
    ///    `received_addresses` entry — finalizes any commands the decode
    ///    deposited into `send_conn.base.world_manager`.
    pub fn process_recv_packets(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
        received_addresses: HashSet<SocketAddr>,
        pending_data_packets: Vec<(SocketAddr, Tick, OwnedBitReader)>,
        server_tick: Tick,
    ) {
        // Step 1: per-address ack drain.
        for address in &received_addresses {
            if let Some(send_conn) = self.send_user_connections.get_mut(address) {
                send_conn.drain_acks(&mut []);
            }
        }

        // Step 2: per-data-packet decode.
        for (address, client_tick, owned_reader) in pending_data_packets {
            let mut reader = owned_reader.borrow();

            let Some(send_conn) = self.send_user_connections.get_mut(&address) else {
                continue;
            };
            let Some(recv_conn) = recv_conns.get_mut(&address) else {
                continue;
            };

            // Recv-side: decode tick-buffered messages.
            if recv_conn
                .tick_buffer
                .read_messages(
                    &self.shared.channel_kinds,
                    &self.shared.message_kinds,
                    &server_tick,
                    &client_tick,
                    send_conn.base.world_manager.entity_converter(),
                    &mut reader,
                )
                .is_err()
            {
                warn!(
                    "Server Error: cannot decode tick-buffered messages from {}",
                    address
                );
                continue;
            }

            // Send-side: decode message/world section.
            if send_conn
                .read_data_section(
                    &self.shared.channel_kinds,
                    &self.shared.message_kinds,
                    &self.shared.component_kinds,
                    self.shared.client_authoritative_entities,
                    client_tick,
                    &mut reader,
                )
                .is_err()
            {
                warn!(
                    "Server Error: cannot decode data section from {}",
                    address
                );
                continue;
            }

            // Arm an ACK-only response (heartbeat-style) so the client
            // gets immediate acknowledgement of this data packet even if
            // the server has nothing to send back this tick.
            send_conn.base.mark_should_send_empty_ack();
        }

        // Step 3: per-address command finalization.
        for address in received_addresses {
            if let Some(send_conn) = self.send_user_connections.get_mut(&address) {
                send_conn.process_received_commands();
            }
        }
    }

    /// Drain `ServerShared::pending_outbound_packets` and emit them via
    /// the send IO. Recv-side handlers (Ping → Pong, Handshake →
    /// ConnectRequest response) enqueue here because they cannot touch
    /// `SendState::send_io` in pipeline mode.
    ///
    /// Relocated from `WorldServer::flush_pending_outbound_packets` in
    /// step 4-F.naia.h so the send thread can drain inline at the top of
    /// `SendState::send_all_packets`.
    pub(crate) fn flush_pending_outbound_packets(&mut self) {
        let pending: Vec<(SocketAddr, OutgoingPacket)> =
            std::mem::take(&mut *self.shared.pending_outbound_packets.lock());
        for (address, packet) in pending {
            if self.send_io.send_packet(&address, packet).is_err() {
                warn!(
                    "Server Error: cannot flush queued outbound packet to {}",
                    address
                );
            }
        }
    }

    /// Periodic heartbeat sweep — emits a heartbeat packet to every
    /// connection that hasn't sent recently. Send-side only (the header
    /// reads the cross-half ACK snapshot via the shared atomic).
    /// Relocated from `WorldServer::handle_heartbeats` in 4-F.naia.h.
    pub(crate) fn handle_heartbeats(&mut self) {
        if !self.heartbeat_timer.ringing() {
            return;
        }
        self.heartbeat_timer.reset();

        let tm_guard = self.shared.time_manager.read();
        let tm: &TimeManager = &*tm_guard;
        for (user_address, send_conn) in self.send_user_connections.iter_mut() {
            if send_conn.base.should_send_heartbeat() {
                Self::send_heartbeat_packet(user_address, send_conn, tm, &mut self.send_io);
            }
        }
    }

    /// Sweep `should_send_empty_ack` flags and emit ACK-only packets.
    /// Relocated from `WorldServer::handle_empty_acks` in 4-F.naia.h.
    pub(crate) fn handle_empty_acks(&mut self) {
        let tm_guard = self.shared.time_manager.read();
        let tm: &TimeManager = &*tm_guard;
        for (user_address, send_conn) in self.send_user_connections.iter_mut() {
            if send_conn.base.should_send_empty_ack() {
                Self::send_heartbeat_packet(user_address, send_conn, tm, &mut self.send_io);
            }
        }
    }

    fn send_heartbeat_packet(
        user_address: &SocketAddr,
        send_conn: &mut SendConnection,
        time_manager: &TimeManager,
        io: &mut SendIo,
    ) {
        let mut writer = BitWriter::new();

        // write header (reads ack snapshot via shared atomic)
        let _header = send_conn.write_header(PacketType::Heartbeat, &mut writer);

        // write server tick
        time_manager.current_tick().ser(&mut writer);
        time_manager.current_tick_instant().ser(&mut writer);

        if io.send_packet(user_address, writer.to_packet()).is_err() {
            // Heartbeat send failure is not fatal: the connection timeout
            // will detect a persistently dead link when heartbeats stop arriving.
            warn!(
                "Server Error: Cannot send heartbeat packet to {}",
                user_address
            );
        }
        send_conn.base.mark_sent();
    }

    /// Pipeline-mode send-half tick body (step 4-F.naia.h).
    ///
    /// Runs every send-side step a tick requires AFTER the coordinator
    /// has called `WorldServer::run_send_preamble` (which performs the
    /// coordination-stage `global_priority` publish + `update_entity_scopes` +
    /// `flush_pending_auth_grants` work it owns).
    ///
    /// Sequence:
    ///   1. Reset the per-tick outgoing-bytes counter.
    ///   2. Drain `pending_outbound_packets` (recv-enqueued handshake /
    ///      pong responses).
    ///   3. `handle_heartbeats` + `handle_empty_acks` — periodic /
    ///      flag-driven ack carriers.
    ///   4. Iris Phase 1+2 — one-shot global dirty scan +
    ///      UserDependent snapshot.
    ///   5. Iris Phase 3A — per-user dirty intersect + serial event
    ///      build.
    ///   6. Iris Phase 3B — parallel per-user packet build (rayon).
    ///   7. Serial flush + re-insert of per-user state, then `send_io`
    ///      dispatch.
    ///
    /// The method is `&mut self`-only — no recv-side borrow needed
    /// because the RTT cross-half read at the top of Phase 3B now
    /// sources from `SendConnection::shared.rtt_avg_ms()` (the atomic
    /// mirror seeded in `new_connection_pair` and refreshed on every
    /// pong by `RecvState::receive`'s `Pong` handler).
    /// C.6 prep — split the per-tick send preamble out of
    /// `send_all_packets` so cyberlith's Send SubApp can run it on the
    /// Send-owned thread before constructing a `SnapshotWorld<E>`,
    /// without first reassembling the WorldServer.
    ///
    /// The preamble:
    ///   1. Drains pending `RoomChange`s from `scope_change_queue` into
    ///      `entity_room_map` + `scope_checks_cache`.
    ///   2. Zeroes the per-tick outgoing-bytes counter.
    ///   3. Flushes queued outbound packets (handshake responses, pong
    ///      responses) that the recv path enqueued because it cannot
    ///      touch `SendState::send_io` in pipeline mode.
    ///   4. Sweeps `handle_heartbeats` + `handle_empty_acks`.
    ///
    /// All four steps need only `SendState` + `Arc<ServerShared>` — no
    /// reassembled WorldServer, no world snapshot. Idempotent within a
    /// tick: setting `preamble_done_this_tick = true` causes the
    /// subsequent `send_all_packets` to skip its inline preamble.
    ///
    /// Backward compat: callers that invoke `send_all_packets` directly
    /// (without first calling this) still get the preamble inline —
    /// `send_all_packets` checks the flag.
    pub fn apply_pending_send_preamble(&mut self) {
        // 1. Drain any SimHandle::room_* pushes that landed between
        // ticks. Mutates entity_room_map + scope_checks_cache; leaves
        // non-RoomChange variants in the queue for the existing per-user
        // scope-evaluation drainer to consume.
        let shared = Arc::clone(&self.shared);
        self.apply_pending_room_changes(&shared.scope_change_queue);

        // 1b. MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — drain
        // any SimHandle::configure_entity_replication pushes. Executes
        // the deferred Send-side leaf ops (publish/unpublish/delegation
        // commands, despawn-from-non-owner, the migration sequence with
        // reserve_first_command, auth fan-out) against
        // `send_user_connections`. Any `EntityEnteredRoom` re-evals it
        // re-pushes are left on the queue for the per-user scope drainer
        // (`apply_pending_scope_changes`) later this tick — exactly as
        // the legacy `publish_entity` re-push behaves.
        self.apply_pending_configure_replication(&shared.scope_change_queue);

        // 2. Zero per-tick byte counter so outgoing_bytes_last_tick()
        // reports only the bytes sent during THIS tick.
        self.send_io.reset_outgoing_bytes_this_tick();

        // 3. 4-F.naia.c.1: flush queued outbound packets (handshake
        // responses, pong responses) that the recv path enqueued
        // because it cannot touch `SendState::send_io` in pipeline mode.
        self.flush_pending_outbound_packets();

        // 4. 4-F.naia.c.2a: periodic send-side maintenance — heartbeats
        // and empty-ack carriers.
        self.handle_heartbeats();
        self.handle_empty_acks();

        self.preamble_done_this_tick = true;
    }

    /// MISSION_SNAPSHOT_DIRTY_TRIM (2026-05-20) — recompute the cross-thread
    /// `needed_entities` bitset from every connection's in-flight
    /// value-reading commands.
    ///
    /// MUST be called AFTER `apply_pending_scope_changes` (so freshly
    /// scope-entered entities have already been `host_init`'d and appear in
    /// each connection's `pending_outbound` set) and BEFORE the Sim-side
    /// `SnapshotWorld` build that reads `needed_snapshot_entries`. Both run
    /// on the same thread inside the park window, so the recompute and the
    /// read never race.
    ///
    /// Result: `needed_entities` = ⋃ over users of "entities whose Spawn /
    /// SpawnWithComponents / InsertComponent isn't yet fully delivered". The
    /// snapshot builder unions this with `global_dirty` (component updates)
    /// to get the full must-include set.
    pub fn refresh_needed_entities(&mut self) {
        let needed = &self.shared.needed_entities;
        needed.clear();

        let handler_arc = self.shared.global_world_manager.read().diff_handler();
        let guard = handler_arc.read().expect("GlobalDiffHandler lock poisoned");
        for send_conn in self.send_user_connections.values() {
            for global_entity in send_conn.base.world_manager.pending_outbound_entities() {
                if let Some(idx) = guard.entity_to_global_idx(&global_entity) {
                    needed.set_bit(idx.as_usize() as u32);
                }
            }
        }
    }

    /// C.6 prep — send a message to the user at `address` without
    /// reassembling the WorldServer.
    ///
    /// `WorldServer::send_message` looks up `user_key → address` via
    /// `sim_handle.user_store`, then dispatches against the send-side
    /// connection and message manager. In the pipeline architecture
    /// (cyberlith Send SubApp permanently holds `SendHandle`), the
    /// caller resolves `user_key → address` via
    /// [`crate::pipeline_actors::SimHandle::user_address`] once and
    /// then calls this method directly — no per-tick WorldServer
    /// reassembly.
    ///
    /// Returns `true` if the message was queued. Returns `false` if
    /// `address` has no matching send connection (user disconnected
    /// between the address lookup and this call, or the address is
    /// stale).
    ///
    /// Internally constructs the per-user entity converter from
    /// `shared.global_world_manager` + the send connection's local
    /// `world_manager`, so messages with `EntityProperty` fields
    /// (e.g. cyberlith's `EntityAssignment`) serialize against the
    /// correct per-user local-entity mapping without the caller
    /// providing a converter.
    pub fn send_message_to_address<C: Channel, M: Message>(
        &mut self,
        address: &std::net::SocketAddr,
        message: &M,
    ) -> bool {
        let container = MessageContainer::new(M::clone_box(message));
        self.send_message_container_to_address(address, &ChannelKind::of::<C>(), container)
    }

    /// Channel-erased sibling of [`send_message_to_address`]. Cyberlith's
    /// generic send paths can call this when they already hold a
    /// `ChannelKind` + `MessageContainer` pair.
    pub fn send_message_container_to_address(
        &mut self,
        address: &std::net::SocketAddr,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) -> bool {
        let channel_settings = self.shared.channel_kinds.channel(channel_kind);
        if !channel_settings.can_send_to_client() {
            panic!("Cannot send message to Client on this Channel");
        }
        let Some(send_conn) = self.send_user_connections.get_mut(address) else {
            return false;
        };
        let gwm = self.shared.global_world_manager.read();
        let mut converter = send_conn.base.world_manager.entity_converter_mut(&*gwm);
        send_conn.base.message_manager.send_message(
            &self.shared.message_kinds,
            &mut converter,
            channel_kind,
            message,
        )
    }

    /// Build and dispatch all outbound packets for this tick.
    ///
    /// If the caller has already invoked [`SendState::apply_pending_send_preamble`]
    /// earlier in the same tick (set via the C.6 prep split), this
    /// skips the inline preamble. Otherwise the preamble runs first
    /// for backward compatibility.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        self.send_all_packets_impl(world, None);
    }

    /// MISSION_TICK_FLOOR Lever 3: active-path transmit that iterates a FROZEN
    /// `global_dirty` snapshot (captured into the send job at snapshot-build
    /// time) instead of the live, concurrently-mutated bitset. This lets the
    /// send worker transmit the *previous* tick's job during the next tick's
    /// Sim without a torn read of `global_dirty`. The deterministic oracle
    /// keeps calling [`Self::send_all_packets`] (frozen = `None`), sending
    /// synchronously against the live bitset within the same tick.
    pub fn send_all_packets_frozen<W: WorldRefType<E> + Sync>(
        &mut self,
        world: W,
        frozen: &FrozenGlobalDirty,
    ) {
        self.send_all_packets_impl(world, Some(frozen));
    }

    fn send_all_packets_impl<W: WorldRefType<E> + Sync>(
        &mut self,
        world: W,
        frozen: Option<&FrozenGlobalDirty>,
    ) {
        #[cfg(feature = "f3_diag")]
        eprintln!(
            "[F3-DIAG naia/SendState] send_all_packets enter send_user_conns={} preamble_done={} scope_done={}",
            self.send_user_connections.len(),
            self.preamble_done_this_tick,
            self.scope_changes_done_this_tick
        );
        // C.6 prep — if the caller already invoked
        // `apply_pending_send_preamble` this tick (via `SendHandle`),
        // skip the inline preamble. Otherwise run it for backward
        // compatibility with all existing callers.
        if !self.preamble_done_this_tick {
            self.apply_pending_send_preamble();
        }
        // Reset the flag for the next tick.
        self.preamble_done_this_tick = false;

        // C.6 prep #6 — entity-scope drain (publishes spawn intents
        // into per-user `send_user_connections`). Auto-call mirrors
        // the preamble flag pattern: if the caller already invoked
        // `apply_pending_scope_changes` this tick, skip. Otherwise
        // run inline. Legacy `WorldServer::send_all_packets` callers
        // see an empty queue here (their `drain_scope_change_queue`
        // already ran during `run_send_preamble`), so this is a
        // wire-byte-identical no-op for them.
        if !self.scope_changes_done_this_tick {
            self.apply_pending_scope_changes(&world);
        }
        self.scope_changes_done_this_tick = false;

        #[cfg(feature = "e2e_debug")]
        {
            crate::server::world_server::SERVER_SEND_ALL_PACKETS_CALLS
                .fetch_add(1, Ordering::Relaxed);
        }
        let now = Instant::now();

        // Collect and shuffle user addresses for fair priority ordering.
        let mut user_addresses: Vec<SocketAddr> =
            self.send_user_connections.keys().copied().collect();
        fastrand::shuffle(&mut user_addresses);

        // ── Iris Phase 1+2: Global dirty scan + UserDependent snapshot ──────────
        #[cfg(feature = "bench_instrumentation")]
        let _iris_p12_t0 = std::time::Instant::now();

        // MISSION_TICK_FLOOR Lever 3: the dirty iteration domain. Both arms
        // materialize the SAME `(global_idx, dirty_words)` order, so the wire is
        // byte-identical to the pre-Lever-3 live path — only the source differs
        // (frozen job copy for the lagged active send; live bitset for the
        // synchronous deterministic oracle).
        let dirty_plan: Vec<(GlobalEntityIndex, Vec<u64>)> = match frozen {
            Some(f) => f
                .dirty_entity_iter()
                .map(|idx| (idx, f.dirty_words(idx).to_vec()))
                .collect(),
            None => self
                .shared
                .global_dirty
                .dirty_entity_iter()
                .map(|idx| {
                    let words = self
                        .shared
                        .global_dirty
                        .dirty_words(idx)
                        .iter()
                        .map(|w| w.load(Ordering::Relaxed))
                        .collect();
                    (idx, words)
                })
                .collect(),
        };

        let mut snapshot_map: SnapshotMap = SnapshotMap::new();
        {
            let handler = self.shared.global_world_manager.read().diff_handler();
            let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
            let idx_to_world = self.shared.idx_to_world.read();
            for (global_idx, dirty_words) in &dirty_plan {
                let global_idx = *global_idx;
                guard.clear_wire_cache_for_entity(global_idx);
                let Some(global_entity) = guard.global_entity_at(global_idx) else { continue; };
                if !self.shared.global_world_manager.read().entity_is_replicating(&global_entity) { continue; }
                let Some(world_entity) = idx_to_world[global_idx.as_usize()] else { continue; };
                if !world.has_entity(&world_entity) { continue; }

                for (word_idx, dirty_word) in dirty_words.iter().enumerate() {
                    let mut word = *dirty_word;
                    while word != 0 {
                        let bit_pos = word.trailing_zeros() as usize;
                        word &= word - 1;
                        let kind_bit = (word_idx * 64 + bit_pos) as u16;
                        let Some(component_kind) = guard.kind_for_bit(kind_bit) else { continue; };
                        if !world.has_component_of_kind(&world_entity, &component_kind) { continue; }

                        if guard.is_component_user_dependent(global_idx, kind_bit).unwrap_or(false) {
                            let snap = world
                                .component_of_kind(&world_entity, &component_kind)
                                .expect("component verified above")
                                .copy_to_box();
                            snapshot_map.insert((global_entity, component_kind), snap);
                        }
                    }
                }
            }
        }

        #[cfg(feature = "bench_instrumentation")]
        crate::server::world_server::bench_iris_counters::NS_PHASE12.fetch_add(
            _iris_p12_t0.elapsed().as_nanos() as u64,
            Ordering::Relaxed,
        );

        // ── Iris Phase 3A: serial — build per-user update_events ────────────────
        #[cfg(feature = "bench_instrumentation")]
        let _iris_p3a_t0 = std::time::Instant::now();

        type UpdateEvents = HashMap<GlobalEntity, (GlobalEntityIndex, HashMap<ComponentKind, u16>)>;
        let mut update_events_by_addr: HashMap<SocketAddr, UpdateEvents> = HashMap::new();
        {
            let handler = self.shared.global_world_manager.read().diff_handler();
            let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
            for user_address in &user_addresses {
                let send_conn = self.send_user_connections.get(user_address).unwrap();
                let mut events: UpdateEvents = HashMap::new();

                // MISSION_TICK_FLOOR Lever 3: same frozen-vs-live split as
                // Phase 1/2, intersected with this user's visibility. Order is
                // identical to the pre-Lever-3 live path.
                let user_dirty_plan: Vec<(GlobalEntityIndex, Vec<u64>)> = match frozen {
                    Some(f) => send_conn
                        .visibility
                        .intersect_dirty_frozen(f)
                        .map(|idx| (idx, f.dirty_words(idx).to_vec()))
                        .collect(),
                    None => send_conn
                        .visibility
                        .intersect_dirty(&*self.shared.global_dirty)
                        .map(|idx| {
                            let words = self
                                .shared
                                .global_dirty
                                .dirty_words(idx)
                                .iter()
                                .map(|w| w.load(Ordering::Relaxed))
                                .collect();
                            (idx, words)
                        })
                        .collect(),
                };
                for (global_idx, dirty_words) in &user_dirty_plan {
                    let global_idx = *global_idx;
                    let Some(global_entity) = guard.global_entity_at(global_idx) else { continue; };
                    #[cfg(feature = "bench_instrumentation")]
                    crate::server::world_server::bench_iris_counters::N_PHASE3_ENTITY_VISITS
                        .fetch_add(1, Ordering::Relaxed);

                    for (word_idx, dirty_word) in dirty_words.iter().enumerate() {
                        let mut word = *dirty_word;
                        while word != 0 {
                            let bit_pos = word.trailing_zeros() as usize;
                            word &= word - 1;
                            let kind_bit = (word_idx * 64 + bit_pos) as u16;
                            let Some(component_kind) = guard.kind_for_bit(kind_bit) else { continue; };
                            #[cfg(feature = "bench_instrumentation")]
                            crate::server::world_server::bench_iris_counters::N_PHASE3_COMPONENT_VISITS
                                .fetch_add(1, Ordering::Relaxed);

                            if send_conn.base.world_manager.is_component_dirty_and_delivered_dense(global_idx, kind_bit) {
                                // fast path
                            } else if send_conn.base.world_manager.diff_mask_is_clear_dense(global_idx, kind_bit) {
                                continue;
                            } else if !send_conn.base.world_manager.is_component_updatable_for_entity(&global_entity, &component_kind) {
                                continue;
                            }

                            events.entry(global_entity)
                                .or_insert_with(|| (global_idx, HashMap::new()))
                                .1.insert(component_kind, kind_bit);
                        }
                    }
                }
                update_events_by_addr.insert(*user_address, events);
            }
        }

        // Pre-populate user_priorities for all users (requires &mut, must be serial).
        for user_address in &user_addresses {
            let send_conn = self.send_user_connections.get(user_address).unwrap();
            self.user_priorities.entry(send_conn.user_key).or_default();
        }

        #[cfg(feature = "bench_instrumentation")]
        crate::server::world_server::bench_iris_counters::NS_PHASE3_BUILD.fetch_add(
            _iris_p3a_t0.elapsed().as_nanos() as u64,
            Ordering::Relaxed,
        );

        // ── Iris Phase 3B: parallel — build packets per user ─────────────────────
        // 4-F.naia.h: RTT is sourced from `SendConnection::shared.rtt_avg_ms()`
        // (the atomic mirror refreshed in `RecvState::receive` on every pong),
        // not from the recv-side `ping_manager.rtt_average`. This removes the
        // last cross-half access from the send path.
        let work: Vec<(SocketAddr, UpdateEvents, SendConnection, UserPriorityState<E>, f32)> =
            user_addresses.iter()
            .filter_map(|addr| {
                let send_conn = self.send_user_connections.remove(addr)?;
                let user_key = send_conn.user_key;
                let user_prio = self.user_priorities.remove(&user_key)
                    .unwrap_or_default();
                let events = update_events_by_addr.remove(addr)
                    .unwrap_or_default();
                let rtt_millis = send_conn.shared.rtt_avg_ms();
                Some((*addr, events, send_conn, user_prio, rtt_millis))
            })
            .collect();

        let channel_kinds     = &self.shared.channel_kinds;
        let message_kinds     = &self.shared.message_kinds;
        let component_kinds   = &self.shared.component_kinds;
        let gwm_guard         = self.shared.global_world_manager.read();
        let gwm: &GlobalWorldManager = &*gwm_guard;
        let global_entity_map_guard = self.shared.global_entity_map.read();
        let global_entity_map: &GlobalEntityMap<E> = &*global_entity_map_guard;
        let idx_to_world_guard = self.shared.idx_to_world.read();
        let idx_to_world: &Vec<Option<E>> = &*idx_to_world_guard;
        let time_manager_guard = self.shared.time_manager.read();
        let time_manager: &TimeManager = &*time_manager_guard;
        let global_priority   = &self.global_priority;
        let snapshot_map_ref  = &snapshot_map;

        #[cfg(feature = "bench_instrumentation")]
        let _iris_p3b_t0 = std::time::Instant::now();

        use rayon::prelude::*;
        let results: Vec<(SocketAddr, Vec<OutgoingPacket>, SendConnection, UserPriorityState<E>)> =
            work.into_par_iter()
            .map(|(addr, mut update_events, mut send_conn, mut user_prio, rtt_millis)| {
                let mut hook = SendStatePriorityHook {
                    global: global_priority,
                    user: &mut user_prio,
                    converter: global_entity_map,
                };

                let initial_entities: Vec<GlobalEntity> = update_events.keys().copied().collect();
                let mut scored: Vec<(GlobalEntity, GlobalEntityIndex, f32, HashMap<ComponentKind, u16>)> =
                    update_events.drain()
                        .map(|(ge, (idx, kinds))| (ge, idx, hook.advance(&ge), kinds))
                        .collect();
                scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                let mut update_list: Vec<(GlobalEntity, GlobalEntityIndex, E, HashMap<ComponentKind, u16>)> =
                    scored.into_iter()
                        .filter_map(|(ge, idx, _, kinds)| {
                            idx_to_world[idx.as_usize()].map(|we| (ge, idx, we, kinds))
                        })
                        .collect();

                let (packets, _) = send_conn.build_all_packets(
                    channel_kinds,
                    message_kinds,
                    component_kinds,
                    &now,
                    &world,
                    global_entity_map,
                    gwm,
                    time_manager,
                    rtt_millis,
                    &mut update_list,
                    snapshot_map_ref,
                );

                let current_tick = time_manager.current_tick();
                let remaining: HashSet<GlobalEntity> =
                    update_list.iter().map(|(ge, _, _, _)| *ge).collect();
                for ge in &initial_entities {
                    if !remaining.contains(ge) {
                        hook.reset_after_send(ge, current_tick as u32);
                    }
                }

                (addr, packets, send_conn, user_prio)
            })
            .collect();

        #[cfg(feature = "bench_instrumentation")]
        crate::server::world_server::bench_iris_counters::NS_PHASE3_SORT.fetch_add(
            _iris_p3b_t0.elapsed().as_nanos() as u64,
            Ordering::Relaxed,
        );

        // ── Serial flush + re-insert ──────────────────────────────────────────────
        for (addr, packets, send_conn, user_prio) in results {
            let user_key = send_conn.user_key;
            #[cfg(feature = "f3_diag")]
            let packet_count = packets.len();
            self.send_user_connections.insert(addr, send_conn);
            self.user_priorities.insert(user_key, user_prio);
            #[cfg(feature = "f3_diag")]
            eprintln!(
                "[F3-DIAG naia/SendState] send_all_packets emit user={:?} addr={:?} packets={}",
                user_key, addr, packet_count
            );
            for packet in packets {
                if self.send_io.send_packet(&addr, packet).is_err() {
                    warn!("Server Error: Cannot send data packet to {}", addr);
                }
            }
        }
        #[cfg(feature = "f3_diag")]
        eprintln!("[F3-DIAG naia/SendState] send_all_packets exit");
    }

    /// Apply pending `RoomChange` events from `scope_change_queue` to
    /// `entity_room_map` + `scope_checks_cache`.
    ///
    /// Must be called before any read of those two structures (i.e. at
    /// the top of `send_all_packets`, before `drain_scope_change_queue`
    /// fires per-user scope re-evaluation).
    ///
    /// Idempotent: re-call drains an empty queue.
    ///
    /// Non-RoomChange variants are left in the queue for the existing
    /// per-user scope-evaluation drainer to consume.
    pub(crate) fn apply_pending_room_changes(
        &mut self,
        scope_change_queue: &Mutex<VecDeque<ScopeChange<E>>>,
    ) {
        let drained: Vec<RoomChange<E>> = {
            let mut q = scope_change_queue.lock();
            let mut out = Vec::with_capacity(q.len());
            let mut keep = VecDeque::with_capacity(q.len());
            for change in q.drain(..) {
                if let ScopeChange::RoomChange(rc) = change {
                    out.push(rc);
                } else {
                    keep.push_back(change);
                }
            }
            *q = keep;
            out
        };

        for change in drained {
            match change {
                RoomChange::UserAdded { room_key, user_key, entities_in_room } => {
                    self.scope_checks_cache.on_user_added_to_room(
                        room_key, user_key, entities_in_room);
                    // C.6 prep #6 — mirror coordination-side user_store room
                    // membership onto SendState so apply_pending_scope_changes
                    // can resolve `user.room_keys()` without coordination-side access.
                    self.user_room_map.entry(user_key)
                        .or_default()
                        .insert(room_key);
                    self.room_users_map.entry(room_key)
                        .or_default()
                        .insert(user_key);
                }
                RoomChange::UserRemoved { room_key, user_key } => {
                    self.scope_checks_cache.on_user_removed_from_room(
                        room_key, user_key);
                    if let Some(set) = self.user_room_map.get_mut(&user_key) {
                        set.remove(&room_key);
                        if set.is_empty() {
                            self.user_room_map.remove(&user_key);
                        }
                    }
                    if let Some(set) = self.room_users_map.get_mut(&room_key) {
                        set.remove(&user_key);
                    }
                }
                RoomChange::EntityAdded { room_key, world_entity, global_entity, users_in_room } => {
                    self.entity_room_map.entity_add_room(&global_entity, &room_key);
                    self.scope_checks_cache.on_entity_added_to_room(
                        room_key, world_entity, users_in_room);
                    self.room_entities_map.entry(room_key)
                        .or_default()
                        .insert(global_entity, world_entity);
                }
                RoomChange::EntityRemoved { room_key, world_entity, global_entity } => {
                    self.entity_room_map.remove_from_room(&global_entity, &room_key);
                    self.scope_checks_cache.on_entity_removed_from_room(
                        room_key, world_entity);
                    if let Some(map) = self.room_entities_map.get_mut(&room_key) {
                        map.remove(&global_entity);
                    }
                }
                RoomChange::RoomDestroyed { room_key, removed_entities } => {
                    for (_world_entity, global_entity) in &removed_entities {
                        self.entity_room_map.remove_from_room(global_entity, &room_key);
                    }
                    self.scope_checks_cache.on_room_destroyed(room_key);
                    // Drop room from mirrors. user_room_map: every user
                    // in the destroyed room loses that membership.
                    if let Some(users) = self.room_users_map.remove(&room_key) {
                        for user_key in users {
                            if let Some(set) = self.user_room_map.get_mut(&user_key) {
                                set.remove(&room_key);
                                if set.is_empty() {
                                    self.user_room_map.remove(&user_key);
                                }
                            }
                        }
                    }
                    self.room_entities_map.remove(&room_key);
                }
            }
        }
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — drain
    /// `ScopeChange::ConfigureReplication` payloads from
    /// `scope_change_queue` and execute their deferred Send-side leaf ops
    /// against `send_user_connections`.
    ///
    /// Each op is a verbatim relocation of the corresponding statement in
    /// `WorldServer::{publish_entity, unpublish_entity,
    /// entity_enable_delegation, enable_delegation_client_owned_entity,
    /// entity_disable_delegation}` — same per-connection call, same
    /// order. The gwm writes already ran (immediately) in
    /// `SimHandle::configure_entity_replication`; this method performs
    /// only the Send-half work, using the pre-transition state captured
    /// into each op's payload (B.2 blocker-1 read-before-write fix).
    ///
    /// Non-`ConfigureReplication` variants are left in the queue for the
    /// other drainers to consume.
    pub(crate) fn apply_pending_configure_replication(
        &mut self,
        scope_change_queue: &Mutex<VecDeque<ScopeChange<E>>>,
    ) {
        let drained: Vec<crate::server::configure_replication::ConfigureCapture<E>> = {
            let mut q = scope_change_queue.lock();
            let mut out = Vec::new();
            let mut keep = VecDeque::with_capacity(q.len());
            for change in q.drain(..) {
                if let ScopeChange::ConfigureReplication(cap) = change {
                    out.push(cap);
                } else {
                    keep.push_back(change);
                }
            }
            *q = keep;
            out
        };

        for capture in drained {
            for op in capture.send_ops {
                self.apply_configure_send_op(op);
            }
        }
    }

    /// Execute a single deferred Send-side `configure_entity_replication`
    /// op. See [`crate::server::configure_replication::ConfigureSendOp`].
    fn apply_configure_send_op(
        &mut self,
        op: crate::server::configure_replication::ConfigureSendOp<E>,
    ) {
        use crate::server::configure_replication::ConfigureSendOp;

        match op {
            ConfigureSendOp::Publish {
                global_entity,
                owner_addr,
            } => {
                // Mirror `publish_entity`'s `send_publish` (the owner is a
                // Client; `server_origin == true`). world_server.rs:2548.
                if let Some(send_conn) = self.send_user_connections.get_mut(&owner_addr) {
                    send_conn
                        .base
                        .world_manager
                        .send_publish(HostType::Server, &global_entity);
                }
            }

            ConfigureSendOp::PublishScopeReeval { global_entity } => {
                // Mirror `publish_entity`'s post-publish scope re-eval
                // (world_server.rs:2570): re-push `EntityEnteredRoom` for
                // every room the now-public entity is in, reading the
                // Send-owned `entity_room_map`. The re-pushed variants are
                // consumed by `apply_pending_scope_changes` later this
                // tick — same as the legacy synchronous behaviour.
                let entity_rooms: Vec<RoomKey> = self
                    .entity_room_map
                    .entity_get_rooms(&global_entity)
                    .map(|rooms| rooms.iter().copied().collect())
                    .unwrap_or_default();
                if !entity_rooms.is_empty() {
                    let mut q = self.shared.scope_change_queue.lock();
                    for room_key in entity_rooms {
                        q.push_back(ScopeChange::EntityEnteredRoom(global_entity, room_key));
                    }
                }
            }

            ConfigureSendOp::Unpublish {
                global_entity,
                owner_addr,
            } => {
                // Mirror `unpublish_entity` (world_server.rs:2601): owner
                // `send_unpublish`, then despawn-from-non-owner.
                if let Some(addr) = owner_addr {
                    if let Some(send_conn) = self.send_user_connections.get_mut(&addr) {
                        send_conn
                            .base
                            .world_manager
                            .send_unpublish(HostType::Server, &global_entity);
                    }
                }

                let entity_idx = self.configure_entity_global_idx(&global_entity);
                for (addr, send_conn) in self.send_user_connections.iter_mut() {
                    if owner_addr == Some(*addr) {
                        continue;
                    }
                    if send_conn.base.world_manager.has_global_entity(&global_entity) {
                        send_conn.base.world_manager.despawn_entity(&global_entity);
                        send_conn.clear_entity_visible(entity_idx);
                    }
                }
            }

            ConfigureSendOp::EnableDelegationFanout {
                global_entity,
                client_origin,
            } => {
                // Mirror `entity_enable_delegation`'s per-connection
                // fan-out (world_server.rs:2664). Snapshot the
                // (user_key, addr) pairs first so we can re-borrow
                // `send_user_connections` mutably inside the loop.
                let users: Vec<(UserKey, SocketAddr)> = self
                    .send_user_connections
                    .keys()
                    .copied()
                    .filter_map(|addr| self.addr_to_user_key(&addr).map(|uk| (uk, addr)))
                    .collect();
                for (user_key, addr) in users {
                    if let Some(client_key) = &client_origin {
                        if &user_key == client_key {
                            continue;
                        }
                    }
                    let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
                        continue;
                    };
                    if !send_conn.base.world_manager.has_global_entity(&global_entity) {
                        continue;
                    }
                    send_conn.base.world_manager.send_enable_delegation(
                        HostType::Server,
                        client_origin.is_some(),
                        &global_entity,
                    );
                }
            }

            ConfigureSendOp::DelegationMigrate {
                global_entity,
                world_entity,
                client_key,
            } => {
                self.apply_delegation_migrate(&global_entity, &world_entity, &client_key);
            }

            ConfigureSendOp::DisableDelegationFanout { global_entity } => {
                // Mirror `entity_disable_delegation`'s per-connection
                // fan-out (world_server.rs:2926).
                let addrs: Vec<SocketAddr> =
                    self.send_user_connections.keys().copied().collect();
                for addr in addrs {
                    let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
                        continue;
                    };
                    if !send_conn.base.world_manager.has_global_entity(&global_entity) {
                        continue;
                    }
                    send_conn
                        .base
                        .world_manager
                        .send_disable_delegation(&global_entity);
                }
            }
        }
    }

    /// Send-side equivalent of `WorldServer::entity_global_idx`
    /// (world_server.rs:3946) — resolve the dense `GlobalEntityIndex`
    /// via the shared diff handler.
    fn configure_entity_global_idx(&self, global_entity: &GlobalEntity) -> GlobalEntityIndex {
        let handler = self.shared.global_world_manager.read().diff_handler();
        let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
        guard
            .entity_to_global_idx(global_entity)
            .unwrap_or(GlobalEntityIndex::INVALID)
    }

    /// Send-side `UserKey` lookup for an address. The sim_handle `user_store`
    /// is not reachable from `SendState`; the per-connection
    /// `SendConnection::user_key` (cached at finalize time) is the
    /// Send-owned source for the address→user mapping.
    fn addr_to_user_key(&self, addr: &SocketAddr) -> Option<UserKey> {
        self.send_user_connections.get(addr).map(|c| c.user_key)
    }

    /// Mirror of `WorldServer::enable_delegation_client_owned_entity`'s
    /// Send-half (world_server.rs:2719) for the deferred drain. The gwm
    /// publicity/ownership/auth writes are performed HERE (not in the
    /// Coord method) because they are interleaved with per-connection
    /// migration state that only Send owns — keeping them adjacent to the
    /// `migrate_entity_remote_to_host` + `host_local_enable_delegation` +
    /// `host_send_migrate_response` sequence preserves legacy ordering and
    /// the subcommand_id=0 MigrateResponse invariant (now made explicit by
    /// D.2.3's `reserve_first_command`).
    fn apply_delegation_migrate(
        &mut self,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_key: &UserKey,
    ) {
        use crate::server::configure_replication::ConfigureWorldOp;

        // Resolve owner + publish-if-needed (mirror world_server.rs:2726).
        let Some(entity_owner) = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
        else {
            panic!("entity should have an owner at this point");
        };
        let owner_user_key = match entity_owner {
            EntityOwner::Client(user_key) => {
                // Publish-after-delegation packet-ordering race: promote to
                // ClientPublic now (gwm write). Legacy
                // `enable_delegation_client_owned_entity` calls
                // `world.entity_publish` in THIS branch only, BEFORE
                // `world.entity_enable_delegation`. Enqueue the Publish
                // world hook now so it lands ahead of the EnableDelegation
                // hook pushed below — preserving legacy world-hook order.
                let result = self
                    .shared
                    .global_world_manager
                    .write()
                    .entity_publish(global_entity);
                if !result {
                    warn!(
                        "apply_delegation_migrate: entity_publish failed for {:?}; aborting",
                        global_entity
                    );
                    return;
                }
                self.shared
                    .pending_world_hooks
                    .lock()
                    .push_back(ConfigureWorldOp::Publish {
                        world_entity: *world_entity,
                    });
                user_key
            }
            EntityOwner::ClientPublic(user_key) => user_key,
            _owner => {
                panic!(
                    "entity should be owned by a public client at this point. Owner is: {:?}",
                    _owner
                );
            }
        };

        let user_key = owner_user_key;
        self.shared
            .global_world_manager
            .write()
            .migrate_entity_to_server(global_entity);

        if self.entity_scope_map.get(&user_key, global_entity).is_none() {
            self.entity_scope_map.insert(user_key, *global_entity, true);
        }

        let Some(addr) = self.addr_for_user_key(&user_key) else {
            panic!("user should exist");
        };
        let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
            panic!("connection does not exist")
        };

        let old_remote_entity = match send_conn
            .base
            .world_manager
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
        {
            Ok(entity) => entity,
            Err(_) => {
                panic!(
                    "Entity must exist as RemoteEntity before delegation: {:?}",
                    global_entity
                );
            }
        };
        let new_host_entity = match send_conn
            .base
            .world_manager
            .migrate_entity_remote_to_host(global_entity)
        {
            Ok(entity) => entity,
            Err(e) => {
                panic!("Failed to migrate entity during delegation: {}", e);
            }
        };
        send_conn
            .base
            .world_manager
            .host_local_enable_delegation(&new_host_entity);
        // MigrateResponse reserved at subcommand_id=0 (D.2.3).
        send_conn.base.world_manager.host_send_migrate_response(
            global_entity,
            &old_remote_entity,
            &new_host_entity,
        );

        self.shared
            .global_world_manager
            .write()
            .entity_enable_delegation(global_entity);

        // Enqueue the EnableDelegation world hook (legacy
        // world_server.rs:2844, AFTER any race-path Publish hook).
        self.shared
            .pending_world_hooks
            .lock()
            .push_back(ConfigureWorldOp::EnableDelegation {
                world_entity: *world_entity,
            });

        // Auth fan-out (mirror world_server.rs:2859). Owner gets Granted
        // iff still in-scope at migration time.
        let owner_in_scope = self
            .entity_scope_map
            .get(client_key, global_entity)
            .copied()
            .unwrap_or(false);
        if owner_in_scope {
            let requester =
                crate::world::server_auth_handler::AuthOwner::from_user_key(Some(client_key));
            let result = self
                .shared
                .global_world_manager
                .write()
                .client_request_authority(global_entity, &requester);
            if result.is_err() {
                panic!("failed to grant authority of client-owned delegated entity to creating user");
            }
            let user_snapshot: Vec<(UserKey, SocketAddr)> = self
                .send_user_connections
                .keys()
                .copied()
                .filter_map(|addr| self.addr_to_user_key(&addr).map(|uk| (uk, addr)))
                .collect();
            for (uk, addr) in user_snapshot {
                let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
                    continue;
                };
                if !send_conn.base.world_manager.has_global_entity(global_entity) {
                    continue;
                }
                let new_status = if uk == *client_key {
                    EntityAuthStatus::Granted
                } else {
                    EntityAuthStatus::Denied
                };
                send_conn
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, new_status);
            }
        }
    }

    /// Resolve a connection address for a `UserKey` via the per-connection
    /// `user_key` cache (the coordination user store is not reachable here).
    fn addr_for_user_key(&self, user_key: &UserKey) -> Option<SocketAddr> {
        self.send_user_connections
            .iter()
            .find_map(|(addr, conn)| {
                if &conn.user_key == user_key {
                    Some(*addr)
                } else {
                    None
                }
            })
    }

    /// C.6 prep #6 — drain entity-scope variants (`EntityEnteredRoom`,
    /// `UserEnteredRoom`, `UserLeftRoom`, `ScopeToggled`) from
    /// `scope_change_queue` and fan them out to user connections.
    ///
    /// Companion to [`apply_pending_send_preamble`] (which only drains
    /// `RoomChange` variants). Together the two methods restore the full
    /// body of the legacy `WorldServer::run_send_preamble` →
    /// `update_entity_scopes` → `drain_scope_change_queue` chain for the
    /// pipeline-mode callers that hold a `SendHandle` directly.
    ///
    /// The entity-scope variants are the ones that publish entities to
    /// user connections: they invoke `host_init_entity` +
    /// `set_entity_visible` (spawn-into-user-connection) and
    /// `despawn_entity` / `pause_entity` (despawn / pause), which is
    /// what makes a freshly-room-added entity show up on clients.
    ///
    /// Needs a `&W: WorldRefType<E>` because the per-(user, entity)
    /// fanout calls `world.has_entity(world_entity)` before spawning —
    /// pipeline-mode callers pass the same `SnapshotWorld<E>` they're
    /// about to feed `send_all_packets`.
    ///
    /// Call between [`apply_pending_send_preamble`] and
    /// [`send_all_packets`] (or omit; `send_all_packets` auto-calls when
    /// the per-tick flag isn't set — see backward-compat note on
    /// `scope_changes_done_this_tick`).
    ///
    /// Idempotent within a tick: setting `scope_changes_done_this_tick`
    /// causes the subsequent `send_all_packets` to skip its auto-call.
    pub fn apply_pending_scope_changes<W: WorldRefType<E>>(&mut self, world: &W) {
        #[cfg(feature = "f3_diag")]
        eprintln!(
            "[F3-DIAG naia/SendState] apply_pending_scope_changes enter queue_len={} send_user_conns={} room_users={:?} user_rooms={:?} room_entities={:?}",
            self.shared.scope_change_queue.lock().len(),
            self.send_user_connections.len(),
            self.room_users_map.iter().map(|(k,v)| (format!("{:?}", k), v.len())).collect::<Vec<_>>(),
            self.user_room_map.iter().map(|(k,v)| (format!("{:?}", k), v.len())).collect::<Vec<_>>(),
            self.room_entities_map.iter().map(|(k,v)| (format!("{:?}", k), v.len())).collect::<Vec<_>>(),
        );
        // Drain all remaining (non-RoomChange) variants. RoomChange
        // entries should already be gone (apply_pending_room_changes
        // ran in the preamble); any that survived (because the caller
        // skipped the preamble) are re-queued for the next preamble
        // tick — they only mutate mirror state, not per-user spawn.
        let drained: Vec<ScopeChange<E>> = {
            let mut q = self.shared.scope_change_queue.lock();
            let mut out = Vec::with_capacity(q.len());
            let mut keep = VecDeque::with_capacity(q.len());
            for change in q.drain(..) {
                match change {
                    // RoomChange + ConfigureReplication are drained by the
                    // preamble (apply_pending_room_changes /
                    // apply_pending_configure_replication). If the caller
                    // skipped the preamble they survive here; re-queue them
                    // for the next preamble tick rather than mishandling.
                    ScopeChange::RoomChange(_) | ScopeChange::ConfigureReplication(_) => {
                        keep.push_back(change)
                    }
                    _ => out.push(change),
                }
            }
            *q = keep;
            out
        };

        // Snapshot `UserKey → SocketAddr` once (avoids repeated
        // `send_user_connections` reverse-scans inside the per-change loop).
        let user_addresses: HashMap<UserKey, SocketAddr> = self
            .send_user_connections
            .iter()
            .map(|(addr, conn)| (conn.user_key, *addr))
            .collect();

        #[cfg(feature = "f3_diag")]
        {
            eprintln!(
                "[F3-DIAG naia/SendState] apply_pending_scope_changes drained={} variants={:?}",
                drained.len(),
                drained.iter().map(|c| match c {
                    ScopeChange::UserEnteredRoom(u, r) => format!("UserEnteredRoom({:?},{:?})", u, r),
                    ScopeChange::UserLeftRoom(u, r) => format!("UserLeftRoom({:?},{:?})", u, r),
                    ScopeChange::EntityEnteredRoom(e, r) => format!("EntityEnteredRoom({:?},{:?})", e, r),
                    ScopeChange::ScopeToggled(u, e, b) => format!("ScopeToggled({:?},{:?},{})", u, e, b),
                    ScopeChange::RoomChange(_) => "RoomChange".to_string(),
                }).collect::<Vec<_>>(),
            );
        }
        for change in drained {
            match change {
                ScopeChange::UserEnteredRoom(user_key, room_key) => {
                    let entity_list: Vec<(GlobalEntity, E)> = self
                        .room_entities_map
                        .get(&room_key)
                        .map(|m| m.iter().map(|(ge, we)| (*ge, *we)).collect())
                        .unwrap_or_default();
                    for (global_entity, world_entity) in &entity_list {
                        self.apply_scope_for_user(
                            world,
                            &user_addresses,
                            &user_key,
                            global_entity,
                            world_entity,
                        );
                    }
                }
                ScopeChange::UserLeftRoom(user_key, room_key) => {
                    // Despawn / pause every entity that was in the
                    // departed room and isn't covered by another shared
                    // room. Mirrors legacy `WorldServer::drain_scope_change_queue`
                    // UserLeftRoom arm.
                    let entity_list: Vec<(GlobalEntity, E)> = self
                        .room_entities_map
                        .get(&room_key)
                        .map(|m| m.iter().map(|(ge, we)| (*ge, *we)).collect())
                        .unwrap_or_default();
                    let user_rooms = self
                        .user_room_map
                        .get(&user_key)
                        .cloned()
                        .unwrap_or_default();
                    let Some(addr) = user_addresses.get(&user_key).copied() else {
                        continue;
                    };
                    let diff_handler_arc =
                        self.shared.global_world_manager.read().diff_handler();
                    let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
                        continue;
                    };
                    for (global_entity, _world_entity) in &entity_list {
                        if let Some(entity_rooms) =
                            self.entity_room_map.entity_get_rooms(global_entity)
                        {
                            if entity_rooms.iter().any(|rk| user_rooms.contains(rk)) {
                                continue;
                            }
                        }
                        if !send_conn.base.world_manager.has_global_entity(global_entity) {
                            continue;
                        }
                        let entity_idx = {
                            let guard = diff_handler_arc.read()
                                .expect("GlobalDiffHandler lock poisoned");
                            guard.entity_to_global_idx(global_entity)
                                .unwrap_or(GlobalEntityIndex::INVALID)
                        };
                        let scope_exit = self
                            .shared
                            .global_world_manager
                            .read()
                            .entity_replication_config(global_entity)
                            .map(|c| c.scope_exit)
                            .unwrap_or(ScopeExit::Despawn);
                        match scope_exit {
                            ScopeExit::Persist => {
                                send_conn.base.world_manager.pause_entity(global_entity);
                                send_conn.clear_entity_visible(entity_idx);
                            }
                            ScopeExit::Despawn => {
                                send_conn.base.world_manager.despawn_entity(global_entity);
                                send_conn.clear_entity_visible(entity_idx);
                            }
                        }
                    }
                }
                ScopeChange::EntityEnteredRoom(global_entity, room_key) => {
                    let user_keys: Vec<UserKey> = self
                        .room_users_map
                        .get(&room_key)
                        .map(|s| s.iter().copied().collect())
                        .unwrap_or_default();
                    // Resolve world_entity once.
                    let world_entity_opt = self
                        .room_entities_map
                        .get(&room_key)
                        .and_then(|m| m.get(&global_entity).copied())
                        .or_else(|| {
                            self.shared
                                .global_entity_map
                                .read()
                                .global_entity_to_entity(&global_entity)
                                .ok()
                        });
                    let Some(world_entity) = world_entity_opt else { continue; };
                    for user_key in &user_keys {
                        self.apply_scope_for_user(
                            world,
                            &user_addresses,
                            user_key,
                            &global_entity,
                            &world_entity,
                        );
                    }
                }
                ScopeChange::ScopeToggled(user_key, global_entity, _is_included) => {
                    let world_entity_opt = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .ok();
                    let Some(world_entity) = world_entity_opt else { continue; };
                    self.apply_scope_for_user(
                        world,
                        &user_addresses,
                        &user_key,
                        &global_entity,
                        &world_entity,
                    );
                }
                ScopeChange::RoomChange(_) | ScopeChange::ConfigureReplication(_) => {
                    // Already filtered above into `keep` — unreachable.
                    unreachable!("RoomChange / ConfigureReplication variants are filtered out");
                }
            }
        }

        self.scope_changes_done_this_tick = true;
    }

    /// Per-(user, entity) scope evaluator. Mirrors the body of legacy
    /// `WorldServer::apply_scope_for_user` (server/src/server/world_server.rs
    /// ~lines 3712-3894), but reads only from `SendState` + `shared` +
    /// the per-call `world` + `user_addresses` snapshot.
    ///
    /// Two contract deviations vs legacy:
    ///
    /// 1. `resource_registry` lives on `CoordinatorState`; not reachable
    ///    from `SendState`. Replicated resources publish via their own
    ///    code path (server/src/server/world_server.rs §RESOURCES_PLAN)
    ///    and are NOT exercised by the cyberlith pipeline's
    ///    `apply_pending_scope_changes` call (the legacy WorldServer
    ///    `send_all_packets` path still runs `drain_scope_change_queue`
    ///    with the live registry). Treating `is_resource` as `false`
    ///    here is safe for the pipeline-mode caller: it only flips the
    ///    "in scope for all users regardless of room" override, which
    ///    is independently published by the resource path.
    ///
    /// 2. On `!world.has_entity(world_entity)` the legacy version
    ///    re-queues a `ScopeToggled(user, entity, true)` so the entity
    ///    enters scope next tick. Same behavior here — push onto
    ///    `scope_change_queue` for the next tick's drain.
    #[allow(clippy::too_many_lines)]
    fn apply_scope_for_user<W: WorldRefType<E>>(
        &mut self,
        world: &W,
        user_addresses: &HashMap<UserKey, SocketAddr>,
        user_key: &UserKey,
        global_entity: &GlobalEntity,
        world_entity: &E,
    ) {
        // Resolve dense GlobalEntityIndex before any mutable borrows.
        let entity_idx = {
            let handler = self.shared.global_world_manager.read().diff_handler();
            let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
            guard
                .entity_to_global_idx(global_entity)
                .unwrap_or(GlobalEntityIndex::INVALID)
        };

        let Some(addr) = user_addresses.get(user_key).copied() else {
            return;
        };
        let Some(send_conn) = self.send_user_connections.get_mut(&addr) else {
            return;
        };
        #[cfg(feature = "f3_diag")]
        eprintln!(
            "[F3-DIAG naia/SendState] apply_scope_for_user user={:?} ge={:?} has_entity_in_world={}",
            user_key,
            global_entity,
            world.has_entity(world_entity)
        );
        // MISSION_SNAPSHOT_DIRTY_TRIM (2026-05-20): under the reorder, scope
        // application runs against the Sim world (entity-existence source of
        // truth) BEFORE the snapshot is built, and `host_init` only fires for
        // entities present here. So a scope-enter for an absent entity should
        // be impossible. Surface it loudly in debug/test builds; keep the
        // bounded re-queue as the release fallback (legit despawn-race / other
        // consumers passing a not-yet-populated world).
        debug_assert!(
            world.has_entity(world_entity),
            "apply_scope_for_user: scope-enter for entity {:?} (user {:?}) but it is \
             absent from the world passed to scope application — see \
             MISSION_SNAPSHOT_DIRTY_TRIM.md §4",
            global_entity,
            user_key,
        );
        if !world.has_entity(world_entity) {
            // Entity not yet spawned in the snapshot world — re-queue
            // for next tick, but only up to `SCOPE_RETRY_MAX` times per
            // (user, entity) pair. See `SCOPE_RETRY_MAX` doc-comment
            // for the 2026-05-19 f3-saga motivation.
            let key = (*user_key, *global_entity);
            let current = self.scope_retry_counts.get(&key).copied().unwrap_or(0);
            match scope_retry_decision(current) {
                None => {
                    warn!(
                        "apply_scope_for_user: dropping ScopeToggled for \
                         user={:?} entity={:?} after {} retries — target \
                         world entity never became available in snapshot \
                         world. See MISSION_USER_ONLY_SEES_SIM §5 (Phase A).",
                        user_key, global_entity, SCOPE_RETRY_MAX,
                    );
                    self.scope_retry_counts.remove(&key);
                }
                Some(next) => {
                    self.scope_retry_counts.insert(key, next);
                    self.shared
                        .scope_change_queue
                        .lock()
                        .push_back(ScopeChange::ScopeToggled(
                            *user_key,
                            *global_entity,
                            true,
                        ));
                }
            }
            return;
        }
        // Success path — `world.has_entity == true`. Reset the retry
        // counter for this (user, entity) pair so a future re-queue
        // starts fresh.
        self.scope_retry_counts.remove(&(*user_key, *global_entity));
        if self
            .shared
            .global_world_manager
            .read()
            .entity_is_public_and_owned_by_user(user_key, global_entity)
        {
            return;
        }
        if matches!(
            self.shared
                .global_world_manager
                .read()
                .entity_owner(global_entity),
            Some(EntityOwner::Client(_)) | Some(EntityOwner::ClientWaiting(_))
        ) {
            return;
        }

        let currently_visible = send_conn.visibility.is_set(entity_idx);
        let is_tracked = send_conn.base.world_manager.has_global_entity(global_entity);
        let currently_paused = is_tracked && !currently_visible;

        let in_common_room = if let Some(entity_rooms) =
            self.entity_room_map.entity_get_rooms(global_entity)
        {
            // user.room_keys() ↔ user_room_map
            let user_rooms = self.user_room_map.get(user_key);
            match user_rooms {
                Some(user_rooms) => entity_rooms.intersection(user_rooms).next().is_some(),
                None => false,
            }
        } else {
            false
        };
        let explicit = self
            .entity_scope_map
            .get(user_key, global_entity)
            .copied();
        // See doc-comment deviation #1: resource_registry not reachable
        // here; treat as `false`. Resources publish via their own path.
        let is_resource = false;
        let entity_is_roomless = self
            .entity_room_map
            .entity_get_rooms(global_entity)
            .is_none();
        let server_owned_roomless_non_resource = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
            .map(|o| o.is_server())
            .unwrap_or(false)
            && !is_resource
            && entity_is_roomless;
        let should_be_in_scope = match explicit {
            Some(true) if server_owned_roomless_non_resource => false,
            Some(in_scope) => in_scope,
            None => is_resource || in_common_room,
        };
        if should_be_in_scope {
            if currently_visible {
                return;
            }
            if currently_paused {
                send_conn.base.world_manager.resume_entity(global_entity);
                send_conn.set_entity_visible(entity_idx);
                return;
            }
            let component_kinds = self
                .shared
                .global_world_manager
                .read()
                .component_kinds(global_entity)
                .unwrap();
            let is_static = self
                .shared
                .global_world_manager
                .read()
                .entity_is_static(global_entity);
            #[cfg(feature = "f3_diag")]
            eprintln!(
                "[F3-DIAG naia/SendState] apply_scope_for_user host_init_entity+set_visible user={:?} ge={:?} is_static={} kinds={}",
                user_key,
                global_entity,
                is_static,
                component_kinds.len()
            );
            send_conn.base.world_manager.host_init_entity(
                global_entity,
                component_kinds,
                &self.shared.component_kinds,
                is_static,
            );
            send_conn.set_entity_visible(entity_idx);

            if !self
                .shared
                .global_world_manager
                .read()
                .entity_is_delegated(global_entity)
            {
                return;
            }
            send_conn.base.world_manager.send_enable_delegation(
                HostType::Server,
                false,
                global_entity,
            );
            if self
                .shared
                .global_world_manager
                .read()
                .entity_has_holder(global_entity)
            {
                let new_status = if self
                    .shared
                    .global_world_manager
                    .read()
                    .user_is_authority_holder(user_key, global_entity)
                {
                    EntityAuthStatus::Granted
                } else {
                    EntityAuthStatus::Denied
                };
                send_conn
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, new_status);
            }
        } else if currently_visible {
            let scope_exit = self
                .shared
                .global_world_manager
                .read()
                .entity_replication_config(global_entity)
                .map(|c| c.scope_exit)
                .unwrap_or(ScopeExit::Despawn);
            match scope_exit {
                ScopeExit::Persist => {
                    send_conn.base.world_manager.pause_entity(global_entity);
                    send_conn.clear_entity_visible(entity_idx);
                }
                ScopeExit::Despawn => {
                    send_conn.base.world_manager.despawn_entity(global_entity);
                    send_conn.clear_entity_visible(entity_idx);
                }
            }
            if let Some(layer) = self.user_priorities.get_mut(user_key) {
                layer.on_scope_exit(world_entity);
            }
        }
    }

    // ====================================================================
    // MISSION_USER_ONLY_SEES_SIM Phase D.3b.2 (2026-05-19) — host-sync
    // worldless ops, relocated from `WorldServer` so the pipeline-mode
    // host-sync drain can run handle-direct (no `run_with_world_server`
    // reassembly). Each mirrors its `WorldServer` namesake field-for-field
    // — identical lock order, identical writes — so the wire output is
    // byte-identical. The `WorldServer::*_worldless` methods are unchanged
    // (they remain the serial-mode path + the parity oracle in tests).
    //
    // Cross-half ownership (audited end-to-end against
    // `world_server.rs:2150/2397/2492`):
    // - `insert_component_worldless` / `remove_component_worldless` touch
    //   `shared.gwm` (writes) + `send_user_connections` (per-connection
    //   fan-out). Both halves live on `SendState` (`send_user_connections`
    //   field + `shared` Arc) — no Coord state needed.
    // - `despawn_entity_worldless` additionally touches
    //   `sim_handle.global_priority_mirror` (eviction) + `sim_handle.room_store`
    //   (room cache cleanup). Those live on `CoordinatorState`, so the
    //   method takes `&mut CoordinatorState<E>`.

    /// Pipeline-mode `is_listening`. Mirrors `WorldServer::is_listening`
    /// (`world_server.rs:251`) — reads only `send_io.is_loaded()`. The
    /// loaded flag is Send-side state, so this lives on `SendState`.
    pub(crate) fn is_listening(&self) -> bool {
        self.send_io.is_loaded()
    }

    /// Pipeline-mode `insert_component_worldless`. Byte-for-byte mirror of
    /// `WorldServer::insert_component_worldless` (`world_server.rs:2397`)
    /// with `excluding_user_opt = None` inlined (the host-sync drain always
    /// passes `None`, so the `sim_handle.user_store` lookup in
    /// `insert_new_component_into_entity_scopes` is dead — omitted here).
    pub(crate) fn insert_component_worldless(
        &mut self,
        world_entity: &E,
        component: &mut dyn Replicate,
    ) {
        let component_kind = component.kind();

        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        if self
            .shared
            .global_world_manager
            .read()
            .has_component_record(&global_entity, &component_kind)
        {
            warn!(
                "Attempted to add component `{:?}` to entity `{:?}` that already has it, this can happen if a delegated entity's auth is transferred to the Server before the Server Adapter has been able to process the newly inserted Component. Skipping this action.",
                component.name(), global_entity,
            );
            return;
        }

        // Inlined `insert_new_component_into_entity_scopes(.., None)`:
        // add component to connections already tracking entity.
        for (_addr, send_conn) in self.send_user_connections.iter_mut() {
            let has_entity = send_conn.base.world_manager.has_global_entity(&global_entity);
            if !has_entity {
                continue;
            }
            send_conn
                .base
                .world_manager
                .insert_component(&global_entity, &component_kind);
        }

        // update in world manager
        self.shared
            .global_world_manager
            .write()
            .insert_component_record(&global_entity, &component_kind);
        self.shared
            .global_world_manager
            .write()
            .insert_component_diff_handler(&self.shared.component_kinds, &global_entity, component);

        // if entity is delegated, convert over
        if self
            .shared
            .global_world_manager
            .read()
            .entity_is_delegated(&global_entity)
        {
            let accessor = self
                .shared
                .global_world_manager
                .read()
                .get_entity_auth_accessor(&global_entity);
            component.enable_delegation(&accessor, None)
        }
    }

    /// Pipeline-mode `remove_component_worldless`. Byte-for-byte mirror of
    /// `WorldServer::remove_component_worldless` (`world_server.rs:2492`).
    pub(crate) fn remove_component_worldless(
        &mut self,
        world_entity: &E,
        component_kind: &ComponentKind,
    ) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Inlined `remove_component_from_all_connections`.
        for (_, send_conn) in self.send_user_connections.iter_mut() {
            if !send_conn.base.world_manager.has_global_entity(&global_entity) {
                continue;
            }
            send_conn
                .base
                .world_manager
                .remove_component(&global_entity, component_kind);
        }

        // cleanup all other loose ends
        self.shared
            .global_world_manager
            .write()
            .remove_component_record(&global_entity, component_kind);
        self.shared
            .global_world_manager
            .write()
            .remove_component_diff_handler(&global_entity, component_kind);
    }

    /// Pipeline-mode `despawn_entity_worldless`. Byte-for-byte mirror of
    /// `WorldServer::despawn_entity_worldless` (`world_server.rs:2150`)
    /// including the inlined `cleanup_entity_replication` +
    /// `despawn_entity_from_all_connections` private helpers. Takes
    /// `&mut CoordinatorState<E>` because the priority-mirror eviction and
    /// room-cache cleanup write Coord-side state.
    pub(crate) fn despawn_entity_worldless(
        &mut self,
        sim_handle: &mut CoordinatorState<E>,
        world_entity: &E,
    ) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return;
        };
        // Priority layer eviction (coordination mirror + send copies + per-user).
        sim_handle.global_priority_mirror.on_despawn(world_entity);
        self.global_priority.on_despawn(world_entity);
        for layer in self.user_priorities.values_mut() {
            layer.on_scope_exit(world_entity);
        }
        // Drop every (*, *, world_entity) tuple from the scope-checks cache.
        self.scope_checks_cache.on_entity_despawned(*world_entity);

        // Inlined `cleanup_entity_replication`.
        self.despawn_entity_from_all_connections(&global_entity);
        self.entity_scope_map.remove_entity(&global_entity);
        if let Some(room_keys) = self.entity_room_map.remove_from_all_rooms(&global_entity) {
            for room_key in room_keys {
                if let Some(room) = sim_handle.room_store.get_mut(&room_key) {
                    room.remove_entity(&global_entity, true);
                }
            }
        }
        self.shared
            .global_world_manager
            .write()
            .remove_entity_diff_handlers(&global_entity);

        self.shared
            .global_world_manager
            .write()
            .remove_entity_record(&global_entity);
        self.shared
            .global_entity_map
            .write()
            .despawn_by_global(&global_entity);
    }

    /// Inlined `WorldServer::despawn_entity_from_all_connections`
    /// (`world_server.rs:2199`). Resolves the global idx, clears
    /// `idx_to_world`, and despawns the entity from every connection that
    /// has it in scope.
    fn despawn_entity_from_all_connections(&mut self, global_entity: &GlobalEntity) {
        let entity_idx = {
            let handler = self.shared.global_world_manager.read().diff_handler();
            let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
            guard
                .entity_to_global_idx(global_entity)
                .unwrap_or(GlobalEntityIndex::INVALID)
        };
        if entity_idx.is_valid() {
            self.shared.idx_to_world.write()[entity_idx.as_usize()] = None;
        }
        for (_, send_conn) in self.send_user_connections.iter_mut() {
            if !send_conn.base.world_manager.has_global_entity(global_entity) {
                continue;
            }
            send_conn.base.world_manager.despawn_entity(global_entity);
            send_conn.clear_entity_visible(entity_idx);
        }
    }
}

/// Per-user priority adapter for Iris Phase 3B (rayon-parallel packet
/// build). Bridges `OutgoingPriorityHook` (keyed by `GlobalEntity`) to
/// the per-user `UserPriorityState<E>` plus the read-only
/// `GlobalPriorityState<E>` layer.
///
/// Relocated from `WorldServer::WorldServerPriorityHook` in 4-F.naia.h
/// so the hook lives next to its sole call site in
/// `SendState::send_all_packets`.
struct SendStatePriorityHook<'a, E: Copy + Eq + Hash + Send + Sync> {
    global: &'a GlobalPriorityState<E>,
    user: &'a mut UserPriorityState<E>,
    converter: &'a GlobalEntityMap<E>,
}

impl<'a, E: Copy + Eq + Hash + Send + Sync> OutgoingPriorityHook
    for SendStatePriorityHook<'a, E>
{
    fn advance(&mut self, entity: &GlobalEntity) -> f32 {
        let Ok(world_entity) = self.converter.global_entity_to_entity(entity) else {
            return 0.0;
        };
        let g = self.global.gain_override(&world_entity).unwrap_or(1.0);
        let u = self.user.gain_override(&world_entity).unwrap_or(1.0);
        self.user.advance(world_entity, g * u)
    }

    fn reset_after_send(&mut self, entity: &GlobalEntity, current_tick: u32) {
        let Ok(world_entity) = self.converter.global_entity_to_entity(entity) else {
            return;
        };
        self.user.reset_after_send(&world_entity, current_tick);
    }
}

// SAFETY: PacketSender is a trait object; concrete impls used by naia
// (UDP and local transports) are Send. The HashMap fields are owned
// outright. UserPriorityState contains POD numeric state.
unsafe impl<E: Copy + Eq + Hash + Send + Sync> Send for SendState<E> {}

/// Phase A of MISSION_USER_ONLY_SEES_SIM (2026-05-19) — pure decision
/// helper for the bounded `ScopeToggled` re-queue counter, factored
/// out of [`SendState::apply_scope_for_user`] so the increment /
/// exhaustion semantics can be unit-tested without fabricating a full
/// `SendState` + connected user.
///
/// `current` is the count BEFORE this attempt (i.e. how many times the
/// entry has already been re-queued). Returns `Some(next)` if the
/// caller should re-queue and store `next` as the new counter value;
/// returns `None` if the cap is reached and the caller should drop the
/// entry + emit a `warn!`.
#[inline]
pub(crate) fn scope_retry_decision(current: u8) -> Option<u8> {
    if current >= SCOPE_RETRY_MAX {
        None
    } else {
        Some(current + 1)
    }
}

/// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — drain the
/// `pending_world_hooks` queue on `shared`, applying each deferred
/// World-side hook op onto `world`.
///
/// Each op maps to a `WorldMutType` hook (`entity_publish` /
/// `entity_unpublish` / `entity_enable_delegation` /
/// `entity_disable_delegation`) — a verbatim relocation of the
/// corresponding `world.*` call in `WorldServer::{publish_entity,
/// unpublish_entity, entity_enable_delegation, entity_disable_delegation}`.
/// Reads the POST-transition gwm (the Coord method already wrote it),
/// exactly as the legacy synchronous path reads it after its own gwm
/// write — so the per-component diff-mutator installation is identical.
///
/// Called from [`crate::pipeline_actors::SimHandle::apply_pending_world_hooks`]
/// and [`crate::SendHandle::apply_pending_world_hooks`].
pub(crate) fn drain_pending_world_hooks<E, W>(shared: &Arc<ServerShared<E>>, world: &mut W)
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    W: WorldMutType<E>,
{
    use crate::server::configure_replication::ConfigureWorldOp;

    let drained: Vec<ConfigureWorldOp<E>> = {
        let mut q = shared.pending_world_hooks.lock();
        q.drain(..).collect()
    };

    for op in drained {
        match op {
            ConfigureWorldOp::Publish { world_entity } => {
                // Mirror world_server.rs:2561.
                let entity_map = shared.global_entity_map.read();
                world.entity_publish(
                    &shared.component_kinds,
                    &*entity_map,
                    &*shared.global_world_manager.read(),
                    &world_entity,
                );
            }
            ConfigureWorldOp::Unpublish { world_entity } => {
                // Mirror world_server.rs:2614.
                world.entity_unpublish(&world_entity);
            }
            ConfigureWorldOp::EnableDelegation { world_entity } => {
                // Mirror world_server.rs:2710 / 2844.
                let entity_map = shared.global_entity_map.read();
                world.entity_enable_delegation(
                    &shared.component_kinds,
                    &*entity_map,
                    &*shared.global_world_manager.read(),
                    &world_entity,
                );
            }
            ConfigureWorldOp::DisableDelegation { world_entity } => {
                // Mirror world_server.rs:2950.
                world.entity_disable_delegation(&world_entity);
            }
        }
    }
}

#[cfg(test)]
mod scope_retry_tests {
    use super::{scope_retry_decision, SCOPE_RETRY_MAX};

    /// The retry counter must accept every attempt up to and including
    /// the (SCOPE_RETRY_MAX - 1)th retry, then reject on the
    /// SCOPE_RETRY_MAX-th attempt. With N=16, retries 0..=15 succeed
    /// (16 re-queues total) and the 16th attempt drops.
    #[test]
    fn scope_retry_decision_caps_at_max() {
        for attempt in 0..SCOPE_RETRY_MAX {
            assert_eq!(
                scope_retry_decision(attempt),
                Some(attempt + 1),
                "attempt {} (under cap) must re-queue",
                attempt,
            );
        }
        assert_eq!(
            scope_retry_decision(SCOPE_RETRY_MAX),
            None,
            "attempt at cap must drop",
        );
        assert_eq!(
            scope_retry_decision(SCOPE_RETRY_MAX + 1),
            None,
            "attempts past cap must also drop (defensive)",
        );
        assert_eq!(
            scope_retry_decision(u8::MAX),
            None,
            "saturated counter must drop without overflow",
        );
    }

    /// N=16 was chosen so that at 25 Hz tick rate the retry window
    /// spans roughly 640 ms. This test pins the constant so any
    /// inadvertent change forces a deliberate review (the rationale
    /// lives on the `SCOPE_RETRY_MAX` doc-comment).
    #[test]
    fn scope_retry_max_is_sixteen() {
        assert_eq!(SCOPE_RETRY_MAX, 16);
    }

    /// First retry from a fresh (count=0) state increments to 1 — the
    /// "happy path" branch every re-queue takes at least once.
    #[test]
    fn first_retry_returns_one() {
        assert_eq!(scope_retry_decision(0), Some(1));
    }
}
