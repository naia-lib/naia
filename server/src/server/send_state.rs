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
    collections::{HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    sync::{atomic::Ordering, Arc},
};

use log::warn;

use naia_shared::{
    BitWriter, ComponentKind, EntityAndGlobalEntityConverter, GlobalEntity, GlobalEntityIndex,
    GlobalEntityMap, GlobalPriorityState, GlobalWorldManagerType, Instant, OutgoingPacket,
    OutgoingPriorityHook, OwnedBitReader, PacketType, Serde, SnapshotMap, Tick, Timer,
    UserPriorityState, WorldRefType,
};

use crate::{
    connection::{io::SendIo, RecvConnection, SendConnection},
    server::ServerShared,
    time_manager::TimeManager,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
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
    /// Iris send-path read. Kept in sync with `coord.global_priority_mirror`
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
    /// recv half, and the coord/serial caller passes a `&mut recv_conns`
    /// borrow for that.
    pub(crate) ping_timer: Timer,

    /// Send half of the transport (step 4-E.2a). Carries the encoder,
    /// outgoing bandwidth monitor, and per-tick byte counter alongside
    /// the `Box<dyn PacketSender>`. Owned here so the send thread has
    /// exclusive mutable access without locking.
    pub send_io: SendIo,

    /// Shared init-only config + cross-thread atomic cells.
    pub shared: Arc<ServerShared<E>>,
}

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
    /// in concurrently because both threads are paused on the coord-stage
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
    /// coord-stage `global_priority` publish + `update_entity_scopes` +
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
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        #[cfg(feature = "e2e_debug")]
        {
            crate::server::world_server::SERVER_SEND_ALL_PACKETS_CALLS
                .fetch_add(1, Ordering::Relaxed);
        }
        let now = Instant::now();

        // Zero per-tick byte counter so outgoing_bytes_last_tick() reports
        // only the bytes sent during THIS tick (readable after send_packets).
        self.send_io.reset_outgoing_bytes_this_tick();

        // 4-F.naia.c.1: flush queued outbound packets (handshake responses,
        // pong responses) that the recv path enqueued because it cannot
        // touch `SendState::send_io` in pipeline mode.
        self.flush_pending_outbound_packets();

        // 4-F.naia.c.2a: periodic send-side maintenance — heartbeats and
        // empty-ack carriers.
        self.handle_heartbeats();
        self.handle_empty_acks();

        // Collect and shuffle user addresses for fair priority ordering.
        let mut user_addresses: Vec<SocketAddr> =
            self.send_user_connections.keys().copied().collect();
        fastrand::shuffle(&mut user_addresses);

        // ── Iris Phase 1+2: Global dirty scan + UserDependent snapshot ──────────
        #[cfg(feature = "bench_instrumentation")]
        let _iris_p12_t0 = std::time::Instant::now();

        let mut snapshot_map: SnapshotMap = SnapshotMap::new();
        {
            let handler = self.shared.global_world_manager.read().diff_handler();
            let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
            let idx_to_world = self.shared.idx_to_world.read();
            for global_idx in self.shared.global_dirty.dirty_entity_iter() {
                guard.clear_wire_cache_for_entity(global_idx);
                let Some(global_entity) = guard.global_entity_at(global_idx) else { continue; };
                if !self.shared.global_world_manager.read().entity_is_replicating(&global_entity) { continue; }
                let Some(world_entity) = idx_to_world[global_idx.as_usize()] else { continue; };
                if !world.has_entity(&world_entity) { continue; }

                for (word_idx, dirty_word) in self.shared.global_dirty.dirty_words(global_idx).iter().enumerate() {
                    let mut word = dirty_word.load(Ordering::Relaxed);
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

                for global_idx in send_conn.visibility.intersect_dirty(&*self.shared.global_dirty) {
                    let Some(global_entity) = guard.global_entity_at(global_idx) else { continue; };
                    #[cfg(feature = "bench_instrumentation")]
                    crate::server::world_server::bench_iris_counters::N_PHASE3_ENTITY_VISITS
                        .fetch_add(1, Ordering::Relaxed);

                    for (word_idx, dirty_word) in self.shared.global_dirty.dirty_words(global_idx).iter().enumerate() {
                        let mut word = dirty_word.load(Ordering::Relaxed);
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
            self.send_user_connections.insert(addr, send_conn);
            self.user_priorities.insert(user_key, user_prio);
            for packet in packets {
                if self.send_io.send_packet(&addr, packet).is_err() {
                    warn!("Server Error: Cannot send data packet to {}", addr);
                }
            }
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
