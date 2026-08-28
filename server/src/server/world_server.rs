use std::{
    any::Any,
    collections::{hash_set::Iter, HashMap},
    hash::Hash,
    net::SocketAddr,
    panic,
    sync::Arc,
    time::Duration,
};

use log::{info, warn};

use naia_shared::{
    AuthorityError, Channel, ChannelKind, ComponentKind, ConnectionStats, DisconnectReason,
    EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError, EntityEvent,
    EntityPriorityMut, EntityPriorityRef, FrozenGlobalDirty, GlobalDirtyBitset, GlobalEntity,
    GlobalEntityIndex, GlobalEntitySpawner, GlobalPriorityState, GlobalRequestId, GlobalResponseId,
    GlobalWorldManagerType, HostType, Instant, LocalEntityAndGlobalEntityConverter, Message,
    MessageContainer, Protocol, Replicate, ReplicatedComponent, Request, ResourceAlreadyExists,
    ResourceRegistry, Response, ResponseReceiveKey, ResponseSendKey, SendPlan,
    SharedGlobalWorldManager, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        connection::new_connection_pair, io::new_io_pair, tick_buffer_messages::TickBufferMessages,
    },
    events::{world_events::WorldEvents, TickEvents},
    request::{GlobalRequestManager, GlobalResponseManager, ResponseSendOutcome},
    room::Room,
    server::scope_checks_cache::ScopeChecksCache,
    transport::{PacketReceiver, PacketSender},
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        entity_room_map::EntityRoomMap, entity_scope_map::EntityScopeMap,
        global_world_manager::GlobalWorldManager, server_auth_handler::AuthOwner,
    },
    NaiaServerError, Publicity, ReplicationConfig, RoomKey, RoomMut, RoomRef, ScopeExit,
    ServerConfig, UserKey, UserMut, UserRef, UserScopeMut, UserScopeRef, WorldUser,
};

use super::{room_store::RoomStore, scope_change::ScopeChange, user_store::UserStore};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use std::sync::atomic::{AtomicUsize, Ordering};
    }
}

/// Timing of `update_entity_scopes` — the per-tick scope-diffing pass that
/// runs at the top of `send_all_packets` before the per-user send loop.
/// Enabled via `bench_instrumentation`.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_scope_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)]
    pub static NS_UPDATE_ENTITY_SCOPES: AtomicU64 = AtomicU64::new(0);

    /// Resets counter to zero.
    pub fn reset() {
        NS_UPDATE_ENTITY_SCOPES.store(0, Ordering::Relaxed);
    }
    /// Returns nanoseconds spent in `update_entity_scopes` this tick.
    pub fn snapshot() -> u64 {
        NS_UPDATE_ENTITY_SCOPES.load(Ordering::Relaxed)
    }
}

/// Timing of the three Iris send-path phases inside `send_all_packets`.
/// Enabled via `bench_instrumentation`.
///
/// - `iris_phase12`: one-shot global dirty scan + UserDependent ECS snapshot
/// - `iris_phase3_build`: per-user intersect_dirty + diff-mask filter → events HashMap (sum)
/// - `iris_phase3_sort`: per-user phase-3B total: score + sort + update_list + build_all_packets (sum)
/// - `iris_phase3_sort_only`: per-user sort call only (subset of phase3_sort)
/// - `iris_phase3_entity_visits`: total (user × entity) pairs entering the dirty_words scan
/// - `iris_phase3_component_visits`: total inner-loop iterations reaching the diff_mask check
#[cfg(feature = "bench_instrumentation")]
pub mod bench_iris_counters {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    #[doc(hidden)]
    pub static NS_PHASE12: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static NS_PHASE3_BUILD: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static NS_PHASE3_SORT: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static NS_PHASE3_SORT_ONLY: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static N_PHASE3_ENTITY_VISITS: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)]
    pub static N_PHASE3_COMPONENT_VISITS: AtomicU64 = AtomicU64::new(0);

    // ── Phase H deferred-followup: send-gate measurement ──────────────────────
    // When true, the Phase-3A send gate (`send_state.rs`) is FORCED to skip the
    // single-lookup fast path (`is_receiver_dirty_and_delivered_fast`) and run
    // the 6+-HashMap `is_component_updatable_for_entity` chain for EVERY visited
    // component — i.e. "close the fast-path bypass". The A/B delta on
    // `steady_state_active` is the recurring per-tick CPU tax of hardening the
    // gate. Off by default; flipped by the measurement harness only.
    #[doc(hidden)]
    pub static FORCE_SLOW_GATE: AtomicBool = AtomicBool::new(false);
    /// When true, the gate cross-checks each fast-path emission against the
    /// ground-truth `is_component_updatable_for_entity` to count leaks/suppressions.
    /// Kept OFF during the (b) timing A/B so that extra call never skews timings.
    #[doc(hidden)]
    pub static MEASURE_LEAK: AtomicBool = AtomicBool::new(false);
    /// Updates emitted via the single-lookup FAST path (dirty+delivered flag).
    #[doc(hidden)]
    pub static N_FAST_EMIT: AtomicU64 = AtomicU64::new(0);
    /// Updates emitted via the SLOW path (`is_component_updatable` == true but the
    /// fast flag was not set — e.g. before the InsertComponent ACK, or forced-slow).
    #[doc(hidden)]
    pub static N_SLOW_EMIT: AtomicU64 = AtomicU64::new(0);
    /// Updates emitted via the FAST path for a component the ground-truth
    /// `is_component_updatable_for_entity` says is NOT yet delivered — i.e. a
    /// pre-delivery ("redundant") update the receiver-side waitlist must buffer.
    #[doc(hidden)]
    pub static N_FASTPATH_LEAK: AtomicU64 = AtomicU64::new(0);
    /// Dirty+visible updates the gate SUPPRESSED because the insert was not yet
    /// delivered (`is_component_updatable` == false on the slow path) — the
    /// bandwidth the gate currently saves vs. a no-gate send.
    #[doc(hidden)]
    pub static N_GATE_SUPPRESSED: AtomicU64 = AtomicU64::new(0);

    /// Resets all Iris phase counters to zero.
    pub fn reset() {
        NS_PHASE12.store(0, Ordering::Relaxed);
        NS_PHASE3_BUILD.store(0, Ordering::Relaxed);
        NS_PHASE3_SORT.store(0, Ordering::Relaxed);
        NS_PHASE3_SORT_ONLY.store(0, Ordering::Relaxed);
        N_PHASE3_ENTITY_VISITS.store(0, Ordering::Relaxed);
        N_PHASE3_COMPONENT_VISITS.store(0, Ordering::Relaxed);
        N_FAST_EMIT.store(0, Ordering::Relaxed);
        N_SLOW_EMIT.store(0, Ordering::Relaxed);
        N_FASTPATH_LEAK.store(0, Ordering::Relaxed);
        N_GATE_SUPPRESSED.store(0, Ordering::Relaxed);
    }
    /// Sets the force-slow-gate toggle (see [`FORCE_SLOW_GATE`]).
    pub fn set_force_slow_gate(v: bool) {
        FORCE_SLOW_GATE.store(v, Ordering::Relaxed);
    }
    /// Sets the leak-measurement toggle (see [`MEASURE_LEAK`]).
    pub fn set_measure_leak(v: bool) {
        MEASURE_LEAK.store(v, Ordering::Relaxed);
    }
    /// Returns `(fast_emit, slow_emit, fastpath_leak, gate_suppressed)`.
    pub fn snapshot_gate() -> (u64, u64, u64, u64) {
        (
            N_FAST_EMIT.load(Ordering::Relaxed),
            N_SLOW_EMIT.load(Ordering::Relaxed),
            N_FASTPATH_LEAK.load(Ordering::Relaxed),
            N_GATE_SUPPRESSED.load(Ordering::Relaxed),
        )
    }
    /// Returns `(phase12_ns, phase3_build_ns, phase3_sort_ns)`.
    pub fn snapshot() -> (u64, u64, u64) {
        (
            NS_PHASE12.load(Ordering::Relaxed),
            NS_PHASE3_BUILD.load(Ordering::Relaxed),
            NS_PHASE3_SORT.load(Ordering::Relaxed),
        )
    }
    /// Returns the sort-call-only nanoseconds (subset of `phase3_sort`).
    pub fn snapshot_sort_only() -> u64 {
        NS_PHASE3_SORT_ONLY.load(Ordering::Relaxed)
    }
    /// Returns `(entity_visits, component_visits)` — iteration counts for the Phase 3 inner loops.
    pub fn snapshot_visits() -> (u64, u64) {
        (
            N_PHASE3_ENTITY_VISITS.load(Ordering::Relaxed),
            N_PHASE3_COMPONENT_VISITS.load(Ordering::Relaxed),
        )
    }
}

#[cfg(feature = "e2e_debug")]
pub static SERVER_RX_FRAMES: AtomicUsize = AtomicUsize::new(0);
/// Tick-buffered data packets that were actually DECODED (applied to the recv
/// connection's tick buffer), counted per packet in `process_recv_packets`.
/// Pairs with [`SERVER_RX_FRAMES`] (packets read off the socket): the ratio
/// `SERVER_RX_PACKETS_DECODED / SERVER_RX_FRAMES` must stay ≈1. A persistent
/// shortfall means arrived command packets are being lost before decode — the
/// exact failure mode of the bounded(1) recv-output-channel drop bug fixed in
/// `plugin_full.rs` (the worker pulled packets off the socket then dropped the
/// `ReceiveOutput` carrying them). Kept as a permanent regression sentinel.
#[cfg(feature = "e2e_debug")]
pub static SERVER_RX_PACKETS_DECODED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_TX_FRAMES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SPAWN_APPLIED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SEND_ALL_PACKETS_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_OUTGOING_CMDS_DRAINED_TOTAL: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_AUTH_GRANTED_EMITTED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_ROOM_MOVE_CALLED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SCOPE_DIFF_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SET_AUTH_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_MSGS_DRAINED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WROTE_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_PKTS_SENT: AtomicUsize = AtomicUsize::new(0);

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct InternalWorldServer<E: Copy + Eq + Hash + Send + Sync> {
    /// Cross-thread shared state (C.3 Phase 4 step 4-A).
    /// Init-only fields (`server_config`, kind tables, `global_dirty`) live
    /// here so pipeline recv/send handles can hold an `Arc<ServerShared<E>>`
    /// without contention. Subsequent steps (4-B onwards) add locked fields.
    pub(crate) shared: Arc<crate::server::ServerShared<E>>,

    /// Recv-thread handle (G-unify Phase 2b): the same `RecvHandle<E>` the
    /// pipelined server holds in its park-slot, here held inline. The fused
    /// engine and the split pipeline are siblings built from the same three
    /// handles — recv op logic lives once on the handle; access its state via
    /// `self.recv.state`.
    pub(crate) recv: crate::RecvHandle<E>,

    /// Send-thread handle (G-unify Phase 2b). See `recv` above. State via
    /// `self.send.state`.
    pub(crate) send: crate::SendHandle<E>,

    /// Coordinator handle (G-unify Phase 2b): bundles `CoordinatorState<E>` +
    /// the shared `Arc<ServerShared<E>>`, so coord-only op logic lives once on
    /// `CoordHandle` and the fused engine delegates to it. State via
    /// `self.sim_handle.state`; the bundled shared (same Arc as `self.shared`)
    /// via `self.sim_handle.shared`.
    pub(crate) sim_handle: crate::CoordHandle<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> InternalWorldServer<E> {
    /// Create a new InternalWorldServer
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let protocol: Protocol = protocol.into();

        let Protocol {
            channel_kinds,
            message_kinds,
            component_kinds,
            tick_interval,
            compression,
            client_authoritative_entities,
            ..
        } = protocol;

        let (recv_io, send_io) = new_io_pair(
            &server_config.connection.bandwidth_measure_duration,
            &compression,
        );

        // +1 for INVALID slot 0; also the size of idx_to_world.
        let capacity = (server_config.max_replicated_entities as usize) + 1;

        // Create GlobalDirtyBitset: capacity = max entity slots (including slot 0 = INVALID),
        // component_count = total registered component kinds from the protocol.
        let global_dirty = {
            let component_count = component_kinds.kind_count() as usize;
            Arc::new(GlobalDirtyBitset::new(capacity, component_count))
        };

        let mut global_world_manager = GlobalWorldManager::new();
        global_world_manager.set_global_dirty(Arc::clone(&global_dirty));
        global_world_manager.init_protocol_kind_count(component_kinds.kind_count());

        let shared = Arc::new(crate::server::ServerShared::new(
            server_config,
            channel_kinds,
            message_kinds,
            component_kinds,
            client_authoritative_entities,
            global_dirty,
            global_world_manager,
            tick_interval,
            capacity,
        ));

        let recv = crate::server::RecvState::new(Arc::clone(&shared), recv_io);

        let heartbeat_interval = shared.server_config.connection.heartbeat_interval;
        let ping_interval = shared.server_config.ping.ping_interval;
        let send = crate::server::SendState {
            send_user_connections: HashMap::new(),
            user_priorities: HashMap::new(),
            global_priority: GlobalPriorityState::new(),
            heartbeat_timer: naia_shared::Timer::new(heartbeat_interval),
            ping_timer: naia_shared::Timer::new(ping_interval),
            send_io,
            shared: Arc::clone(&shared),
            // Phase A.3 — relocated from CoordinatorState.
            entity_room_map: EntityRoomMap::new(),
            entity_scope_map: EntityScopeMap::new(),
            scope_checks_cache: ScopeChecksCache::new(),
            preamble_done_this_tick: false,
            // C.6 prep #6 — entity-scope drainer state + mirrors.
            scope_changes_done_this_tick: false,
            user_room_map: HashMap::new(),
            room_users_map: HashMap::new(),
            room_entities_map: HashMap::new(),
            // Phase A of MISSION_USER_ONLY_SEES_SIM (2026-05-19) —
            // bounded per-entry retry counter for `ScopeToggled`
            // re-queues issued by `apply_scope_for_user`.
            scope_retry_counts: HashMap::new(),
        };

        let sim_handle = crate::server::CoordinatorState {
            user_store: UserStore::new(),
            room_store: RoomStore::new(),
            global_request_manager: GlobalRequestManager::new(),
            global_response_manager: GlobalResponseManager::new(),
            global_priority_mirror: GlobalPriorityState::new(),
            user_priority_staging: std::collections::HashMap::new(),
            pending_scope_ledger_ops: Vec::new(),
            pending_resource_ops: Vec::new(),
            pending_lifecycle_ops: Vec::new(),
            pending_authority_ops: Vec::new(),
            pending_outbound_message_ops: Vec::new(),
            resource_registry: ResourceRegistry::new(),
            historian: None,
        };

        Self {
            recv: crate::RecvHandle { state: recv },
            send: crate::SendHandle { state: send },
            sim_handle: crate::CoordHandle {
                state: sim_handle,
                shared: Arc::clone(&shared),
            },
            shared,
        }
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.send.state.send_io.is_loaded()
    }

    pub(crate) fn entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        // 4-E.2c: GlobalEntityMap now lives behind ServerShared::global_entity_map
        // RwLock. The trait impl on InternalWorldServer below acquires its own brief
        // read guards, so callers get a converter that does the right thing
        // without exposing the inner map.
        self
    }

    /// Attaches external sender/receiver I/O handles (used by adapter crates and test harnesses).
    pub fn io_load(&mut self, sender: Box<dyn PacketSender>, receiver: Box<dyn PacketReceiver>) {
        self.recv.state.recv_io.load(receiver);
        self.send.state.send_io.load(sender);
    }

    /// Registers a newly-accepted user so the world server can track their scope (adapter use only).
    pub fn receive_user(&mut self, user_key: UserKey, user_addr: SocketAddr) {
        self.sim_handle
            .state
            .user_store
            .insert(user_key, WorldUser::new(user_addr));
        self.sim_handle
            .state
            .user_store
            .register_disconnected(user_addr, user_key);
        // Auto-include of Replicated Resources happens in
        // `finalize_connection` — that's the point at which a Connection
        // exists in `user_connections` (required by `apply_scope_for_user`
        // to actually push spawn messages).
    }

    fn finalize_connection(&mut self, user_key: &UserKey, user_address: &SocketAddr) {
        if !self.sim_handle.state.user_store.contains(user_key) {
            warn!("unknown user is finalizing connection...");
            return;
        };

        let (recv_conn, send_conn) = new_connection_pair(
            &self.shared.server_config.connection,
            &self.shared.server_config.ping,
            user_address,
            user_key,
            &self.shared.channel_kinds,
            &*self.shared.global_world_manager.read(),
            self.shared.server_config.max_replicated_entities as usize,
        );

        // 4-E.2e: recv-side insertion happens directly (same thread
        // owns recv_user_connections). The send half is queued via
        // `SendStateUpdate::ConnectionAdded` — in serial mode the queue
        // drains at `InternalWorldServer::receive`'s tail before the user can
        // observe any difference; in pipeline mode the coordinator
        // drains at step 6.5 so the recv thread never touches the
        // send-side map directly.
        self.recv
            .state
            .recv_user_connections
            .insert(*user_address, recv_conn);
        self.shared.pending_send_state_updates.lock().push(
            crate::server::SendStateUpdate::ConnectionAdded(*user_address, Box::new(send_conn)),
        );

        if self.send.state.send_io.bandwidth_monitor_enabled() {
            self.recv.state.recv_io.register_client(user_address);
            self.send.state.send_io.register_client(user_address);
        }

        // 4-E.2e: drain ConnectionAdded inline so any subsequent
        // read_data_packet in the same recv cycle, or the resource
        // auto-scope below, can find the new `SendConnection` in
        // `send.send_user_connections`. In pipeline mode (4-F+) this
        // drain moves to the coordinator at step 6.5; here in serial
        // mode the recv and coordinator threads are one, so doing it
        // immediately is equivalent and preserves single-recv-cycle
        // semantics.
        self.commit_pending_send_state_updates();

        // Replicated Resources auto-scope: now that the connection
        // exists in the user_connections maps, scope-include every
        // currently-existing resource entity for this user. Without
        // this step, late-joining clients never receive resources (the
        // room gate bypass in `apply_scope_for_user` requires a
        // matching SendConnection to exist; resource entities
        // themselves never enter rooms).
        let resource_entities = self.resource_entities();
        for world_entity in resource_entities {
            self.user_scope_set_entity(user_key, &world_entity, true);
        }

        self.recv
            .state
            .incoming_world_events
            .push_connection(user_key);
    }

    /// Maintain connection with a client and read all incoming packet data
    /// (serial-mode wrapper, step 4-F.naia.c.2b).
    ///
    /// The recv-only socket loop now lives on [`RecvState::receive`];
    /// the cross-half decode + per-address drain + command finalization
    /// lives on [`SendState::process_recv_packets`]. This wrapper glues
    /// them together with the coordination-stage `drain_pending_handshakes`
    /// step in between (which needs `sim_handle.user_store`).
    pub fn receive_all_packets(&mut self) {
        // Send-side bandwidth tick (the recv-side counterpart fires inside
        // `RecvState::receive`).
        self.send.state.send_io.tick_bandwidth_monitor();

        // 4-F.naia.c.2c: periodic ping dispatch. Method lives on
        // `SendState`; recv-side `ping_manager` access flows in via the
        // `&mut recv_user_connections` borrow. Pipeline coordinator
        // calls the same method directly on `SendHandle`.
        self.send
            .state
            .send_pings(&mut self.recv.state.recv_user_connections);
        // 4-F.naia.c.2a: handle_heartbeats + handle_empty_acks fire at
        // the top of `send_all_packets`, not here.

        // 1. Recv-only socket loop + recv-side periodic disconnect scan.
        self.recv.state.receive();

        // 2. Coord-stage handshake drain — must happen before the
        // cross-half post-pass so newly-finalized SendConnections exist
        // when the data decoder looks them up.
        self.drain_pending_handshakes();

        // 3. Cross-half post-pass: per-data-packet decode + per-address
        // process_received_commands. The ACK drain (L3 seam Step 5) moved OUT of
        // `process_recv_packets` so the pipeline coordinator does NOT drain — its
        // send worker drains in its preamble (single-owner `sent_updates`). The
        // monolithic synchronous server has no send worker, so it drains here, at
        // the same tick position the old `process_recv_packets` step-1 drain held.
        self.send.state.drain_all_acks();
        let received_addresses = std::mem::take(&mut self.recv.state.received_addresses);
        let pending_data_packets = std::mem::take(&mut self.recv.state.pending_data_packets);
        let server_tick = self.shared.time_manager.read().current_tick();
        self.send.state.process_recv_packets(
            &mut self.recv.state.recv_user_connections,
            received_addresses,
            pending_data_packets,
            server_tick,
        );
    }

    /// 4-F.naia.c.1: coordinator-stage drain of `shared.pending_handshakes`.
    /// Recv path pushes addresses on incoming ClientConnectRequest packets;
    /// this method finalizes each one (lookup user_key via
    /// `sim_handle.user_store.take_disconnected`, build connection pair, register
    /// shared atomic, queue ConnectionAdded). Idempotent on repeated pushes
    /// because `take_disconnected` returns None on the second call (spec
    /// Option C-2).
    ///
    /// Called inline at the tail of `receive_all_packets` in serial mode;
    /// in pipeline mode (4-F.cyberlith.e), the coordinator calls this after
    /// `RecvHandle::receive` returns and before `run_send_preamble`.
    pub(crate) fn drain_pending_handshakes(&mut self) {
        let pending: Vec<SocketAddr> = std::mem::take(&mut *self.shared.pending_handshakes.lock());
        for address in pending {
            let Some(user_key) = self.sim_handle.state.user_store.take_disconnected(&address)
            else {
                // Repeat Handshake retry (already finalized) — silently
                // skip. The Connect Response was already queued by the
                // recv path so the client will observe the retry as
                // acknowledged.
                continue;
            };
            self.finalize_connection(&user_key, &address);
        }
    }

    /// Decodes and applies all buffered incoming packets for this frame.
    // MISSION_PIPELINE_API_BOUNDARY G8b: takes `world: &mut W` (not by value) so a
    // caller applying SEVERAL `ReceiveOutput`s per tick (the worker-shape channel
    // burst drained by `PipelinedWorldServer::receive`) can reborrow one world across
    // them — a `WorldMutType` proxy is single-use by value. Byte-identical: this
    // body never moved `world`, only ever borrowed it `&mut`.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, world: &mut W, now: &Instant) {
        self.process_disconnects(world);

        let addresses = std::mem::take(&mut self.recv.state.addrs_with_new_packets);
        for address in addresses {
            self.process_packets(&address, world, now);
        }
    }

    /// Drain `ServerShared::pending_send_state_updates` and apply each
    /// variant to `self.send.state` (step 4-E.2e).
    ///
    /// In serial mode this is called inline immediately after
    /// `finalize_connection` / `user_delete` push their updates so
    /// observable behavior matches the pre-4-E.2e direct-write path.
    /// In pipeline mode (4-F+), the coordinator calls this at step 6.5
    /// between recv and send phases — the recv thread must not touch
    /// `send.send_user_connections` directly.
    pub fn commit_pending_send_state_updates(&mut self) {
        use crate::server::send_state_update::SendStateUpdate;
        // Swap the queue out under the lock so we don't hold it across
        // the apply loop (the apply step takes no other locks but the
        // pattern stays cheap and lock-order-friendly).
        let pending: Vec<SendStateUpdate> = {
            let mut guard = self.shared.pending_send_state_updates.lock();
            std::mem::take(&mut *guard)
        };
        for update in pending {
            match update {
                SendStateUpdate::ConnectionAdded(addr, send_conn) => {
                    self.send
                        .state
                        .send_user_connections
                        .insert(addr, *send_conn);
                }
                SendStateUpdate::ConnectionRemoved(addr) => {
                    self.send.state.send_user_connections.remove(&addr);
                }
            }
        }
    }

    /// Drains and returns all pending world events for this frame.
    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        std::mem::replace(
            &mut self.recv.state.incoming_world_events,
            WorldEvents::<E>::new(),
        )
    }

    /// Serial-equivalent pipeline-coordinator entry point (step 4-F.cyberlith.d).
    ///
    /// Runs the full receive sequence — [`receive_all_packets`](Self::receive_all_packets)
    /// (network IO + handshake finalize + cross-half decode of data packets),
    /// followed by [`process_all_packets`](Self::process_all_packets) (which
    /// applies the decoded `EntityEvent`s to `world` and populates
    /// `incoming_world_events` with the resulting Spawn / Insert / Update /
    /// Despawn entries) — and then drains the accumulated world + tick events
    /// into a [`ReceiveOutput`].
    ///
    /// Without the `process_all_packets` call, all client-driven world
    /// mutations (delegated spawns, client-authoritative component inserts,
    /// remote despawns, queued disconnects) would silently fail to fire their
    /// matching `incoming_world_events` entries — adapter-side callers (e.g.
    /// `naia_bevy_server::apply_receive_output`) would then see an empty
    /// world-events stream and never fan out the corresponding Bevy events
    /// even though the binary packets decoded successfully.
    ///
    /// Pipeline-mode (4-F.cyberlith.e) callers that run the recv phase on a
    /// background thread should use [`receive_all_packets`](Self::receive_all_packets)
    /// + [`process_all_packets`](Self::process_all_packets) +
    /// [`take_world_events`](Self::take_world_events) directly so the recv
    /// loop can run without holding `world`.
    pub fn receive_with_world<W: WorldMutType<E>>(
        &mut self,
        mut world: W,
    ) -> super::receive_output::ReceiveOutput<E> {
        let now = Instant::now();
        self.receive_all_packets();
        self.process_all_packets(&mut world, &now);
        let world_events = self.take_world_events();
        let mut tick_events = self.take_tick_events(&now);
        let pending_ticks: Vec<Tick> = tick_events.read::<crate::events::TickEvent>().collect();
        super::receive_output::ReceiveOutput {
            world_events,
            pending_ticks,
            received_addresses: std::collections::HashSet::new(),
            pending_data_packets: Vec::new(),
        }
    }

    /// Receive-only step that skips world mutation.
    ///
    /// Runs [`receive_all_packets`](Self::receive_all_packets) and then drains
    /// the accumulated world events into a [`ReceiveOutput`].
    ///
    /// [`process_all_packets`](Self::process_all_packets) is NOT called here
    /// because it requires a `World` reference. The world-events stream
    /// therefore contains only entries that `receive_all_packets` populates
    /// directly (e.g. connect events fired by `finalize_connection`); any
    /// entries that would have come from decoded data packets (spawn / insert
    /// / despawn / queued-disconnect) are absent. Most callers want
    /// [`receive_with_world`](Self::receive_with_world) instead.
    pub fn receive(&mut self) -> super::receive_output::ReceiveOutput<E> {
        self.receive_all_packets();
        let world_events = self.take_world_events();
        // Advance the tick clock and collect any ticks that fired during this
        // recv phase. In the pipeline-coordinator architecture (Phase 4), the
        // bevy adapter's `translate_tick_events` system is removed from
        // `Update`, so this is the only place that drives `recv_server_tick`.
        let mut tick_events = self.take_tick_events(&Instant::now());
        let pending_ticks: Vec<Tick> = tick_events.read::<crate::events::TickEvent>().collect();
        // 4-F.naia.c.2b: in the serial path `receive_all_packets` already
        // drained `received_addresses` + `pending_data_packets` into the
        // inline `SendState::process_recv_packets` call, so the
        // ReceiveOutput surface for these is empty here. Pipeline-mode
        // `RecvHandle::receive` populates them.
        super::receive_output::ReceiveOutput {
            world_events,
            pending_ticks,
            received_addresses: std::collections::HashSet::new(),
            pending_data_packets: Vec::new(),
        }
    }

    /// Consume this `InternalWorldServer` and return the three pipeline pieces
    /// (step 4-E.2f). `CoordinatorState<E>` is handed back to the caller
    /// directly so the bevy pipeline adapter can stash it inside
    /// `ServerImpl::WorldOnly` (or equivalent) and orchestrate the
    /// recv/send threads around it per the 12-step §8 sequence.
    ///
    /// The `Arc<ServerShared<E>>` clone needed to wire the two halves
    /// back together (or to read shared state from the coordinator
    /// thread) can be cloned from either handle's `state.shared` before
    /// passing the handles off to their threads.
    pub fn into_pipeline_handles(
        self,
    ) -> (
        super::coord_state::CoordinatorState<E>,
        super::pipeline_handles::RecvHandle<E>,
        super::pipeline_handles::SendHandle<E>,
    ) {
        let (sim_handle, recv_state, send_state) = self.into_pipeline_states();
        (
            sim_handle,
            super::pipeline_handles::RecvHandle { state: recv_state },
            super::pipeline_handles::SendHandle { state: send_state },
        )
    }

    /// Reassemble a `InternalWorldServer<E>` from its three pipeline pieces
    /// (step 4-E.2f). Used by callers that need to drive the full
    /// `receive_all_packets` / `send_all_packets` lifecycle through the
    /// existing methods after a structural split — primarily the
    /// `pipeline_recv_send_independent` smoke test and any 4-F adapter
    /// path that still needs the monolithic `InternalWorldServer` surface.
    ///
    /// The `Arc<ServerShared<E>>` is recovered from `recv.shared`
    /// (the same Arc clone also lives on `send.shared`).
    pub fn from_pipeline_states(
        sim_handle: super::coord_state::CoordinatorState<E>,
        recv: super::recv_state::RecvState<E>,
        send: super::send_state::SendState<E>,
    ) -> Self {
        let shared = Arc::clone(&recv.shared);
        Self {
            recv: crate::RecvHandle { state: recv },
            send: crate::SendHandle { state: send },
            sim_handle: crate::CoordHandle {
                state: sim_handle,
                shared: Arc::clone(&shared),
            },
            shared,
        }
    }

    /// Consume this `InternalWorldServer` into the field-level pipeline states
    /// (step 4-E). Dissolves the per-user `Connection` wrappers into the
    /// recv and send halves, populating `RecvState::recv_user_connections`
    /// and `SendState::send_user_connections` respectively. Returns the
    /// two states along with any non-pipeline-owned residual that the
    /// caller may need (currently the coordinator-only state on the
    /// world_server such as room_store, entity_room_map, etc., still
    /// lives on `Self` and is *not* migrated here — step 4-F's coordinator
    /// keeps the residual `InternalWorldServer` around for borrow-API surface
    /// continuity per the §8 "Refined architecture" note; this method
    /// returns only the two thread-side states needed by the recv/send
    /// threads).
    ///
    /// **Step 4-F integration:** the cyberlith coordinator calls this
    /// during `GameCell::init()` to harvest `RecvState` + `SendState`
    /// for the recv/send threads, while keeping the residual `InternalWorldServer`
    /// alive as the `Server` SystemParam target. The split happens once
    /// at startup; thereafter the two states evolve independently
    /// (subject to the handoff queues maintained on `ServerShared`).
    pub fn into_pipeline_states(
        self,
    ) -> (
        super::coord_state::CoordinatorState<E>,
        super::recv_state::RecvState<E>,
        super::send_state::SendState<E>,
    ) {
        // 4-E.2d: `recv_user_connections` and `send_user_connections` are
        // authoritative in both serial and pipeline modes — no dissolution
        // step needed here, just move the substates out.
        (self.sim_handle.state, self.recv.state, self.send.state)
    }

    /// Advances the tick clock and returns any new tick events for this frame.
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        // 4-E.2b: the single write-guard site for `time_manager`. Hold the
        // write guard only long enough to advance the clock + read the
        // resulting tick(s), then drop it before touching `self.recv.state`.
        // Drain ALL ticks due at `now` (grid catch-up).
        {
            let mut tm = self.shared.time_manager.write();
            while tm.recv_server_tick(now) {
                let tick = tm.current_tick();
                self.recv.state.incoming_tick_events.push_tick(tick);
            }
        }
        std::mem::replace(&mut self.recv.state.incoming_tick_events, TickEvents::new())
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        message: &M,
    ) -> Result<(), NaiaServerError> {
        let container = MessageContainer::new(M::clone_box(message));
        self.send_message_inner(user_key, &ChannelKind::of::<C>(), container)
    }

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    fn send_message_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) -> Result<(), NaiaServerError> {
        let channel_settings = self.shared.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_client() {
            panic!("Cannot send message to Client on this Channel");
        }

        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            #[cfg(feature = "f3_diag")]
            eprintln!(
                "[F3-DIAG naia/InternalWorldServer] send_message_inner user_not_found_in_user_store user={:?} channel={:?}",
                user_key, channel_kind
            );
            return Err(NaiaServerError::UserNotFound);
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            #[cfg(feature = "f3_diag")]
            eprintln!(
                "[F3-DIAG naia/InternalWorldServer] send_message_inner no_send_conn user={:?} addr={:?} channel={:?}",
                user_key, user.address(), channel_kind
            );
            return Err(NaiaServerError::UserNotFound);
        };
        #[cfg(feature = "f3_diag")]
        eprintln!(
            "[F3-DIAG naia/InternalWorldServer] send_message_inner enqueue user={:?} addr={:?} channel={:?}",
            user_key, user.address(), channel_kind
        );
        let gwm = self.shared.global_world_manager.read();
        let mut converter = send_conn.base.world_manager.entity_converter_mut(&*gwm);
        let accepted = send_conn.base.message_manager.send_message(
            &self.shared.message_kinds,
            &mut converter,
            channel_kind,
            message,
        );
        if accepted {
            Ok(())
        } else {
            Err(NaiaServerError::MessageQueueFull)
        }
    }

    /// Sends a message to all connected users using the given channel.
    ///
    /// Per-user send failures are silently discarded. If a particular user's
    /// send fails (e.g. their connection was just dropped), the error is ignored
    /// and the remaining users still receive the message. Callers that need
    /// per-user delivery guarantees should use `send_message` in a loop.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = M::clone_box(message);
        self.broadcast_message_inner(&ChannelKind::of::<C>(), cloned_message);
    }

    fn broadcast_message_inner(
        &mut self,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        // Wrap once in Arc — each per-user clone is a refcount increment, not
        // a heap allocation. At 1,262 CCU this drops from 1,262 clone_box()
        // allocations per broadcast to 1.
        let container = MessageContainer::new(message_box);
        let user_keys: Vec<UserKey> = self.user_keys().to_vec();
        for user_key in user_keys {
            let _ = self.send_message_inner(&user_key, channel_kind, container.clone());
        }
    }

    /// Sends a typed request to the given user and returns a key for receiving the response.
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        let cloned_request = Q::clone_box(request);
        let id = self.send_request_inner(user_key, &ChannelKind::of::<C>(), cloned_request)?;
        Ok(ResponseReceiveKey::new(id))
    }

    fn send_request_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        request_box: Box<dyn Message>,
    ) -> Result<GlobalRequestId, NaiaServerError> {
        let channel_settings = self.shared.channel_kinds.channel(channel_kind);

        if !channel_settings.can_request_and_respond() {
            panic!("Requests can only be sent over Bidirectional, Reliable Channels");
        }

        let request_id = self
            .sim_handle
            .state
            .global_request_manager
            .create_request_id(user_key);

        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            warn!("user does not exist");
            return Err(NaiaServerError::Message("user does not exist".to_string()));
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            warn!("currently not connected to user");
            return Err(NaiaServerError::Message(
                "currently not connected to user".to_string(),
            ));
        };
        let gwm = self.shared.global_world_manager.read();
        let mut converter = send_conn.base.world_manager.entity_converter_mut(&*gwm);

        let message = MessageContainer::new(request_box);
        if !send_conn.base.message_manager.send_request(
            &self.shared.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            message,
        ) {
            // Queue-depth cap reached: nothing was enqueued. Report it rather than
            // handing back an id whose response will never arrive.
            return Err(NaiaServerError::Message(
                "channel send queue full; retry later".to_string(),
            ));
        }

        Ok(request_id)
    }

    /// Sends a Response for a given Request. Returns whether or not was successful.
    ///
    /// `false` means the response was NOT enqueued — either the request is no longer
    /// routable (user gone) or the channel's reliable send queue is full. In the
    /// queue-full case the `response_key` is still valid, so the caller should hold
    /// the response and retry on a later frame; dropping it instead strands whatever
    /// the requester is waiting on, since a request gets at most one response.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.try_send_response(response_key, response) == ResponseSendOutcome::Sent
    }

    /// Like [`Self::send_response`], but distinguishes a *transient* refusal from a
    /// *permanent* one.
    ///
    /// `send_response`'s single `bool` collapses two opposite meanings: "the queue is
    /// momentarily full, hold this and retry" and "this response can never be
    /// delivered". A caller that parks refused responses in a FIFO to preserve
    /// ordering cannot act correctly on that bool — retrying forever head-of-line
    /// blocks every later response behind an undeliverable one, while discarding
    /// strands requesters whose response was merely backpressured. Callers that hold
    /// and retry must use this method; the `bool` form remains for callers that
    /// fire-and-forget.
    pub fn try_send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> ResponseSendOutcome {
        let response_id = response_key.response_id();

        let cloned_response = S::clone_box(response);

        self.send_response_inner(&response_id, cloned_response)
    }

    fn send_response_inner(
        &mut self,
        response_id: &GlobalResponseId,
        response_box: Box<dyn Message>,
    ) -> ResponseSendOutcome {
        // Peek, don't consume: if the enqueue is refused below, the mapping must
        // survive so the caller can retry with the same key.
        let Some((user_key, channel_kind, local_response_id)) = self
            .sim_handle
            .state
            .global_response_manager
            .peek_response_id(response_id)
        else {
            // No routing for this id: already answered, or the request is gone.
            return ResponseSendOutcome::Undeliverable;
        };
        let Some(user) = self.sim_handle.state.user_store.get(&user_key) else {
            return ResponseSendOutcome::Undeliverable;
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            return ResponseSendOutcome::Undeliverable;
        };
        let accepted = {
            let gwm = self.shared.global_world_manager.read();
            let mut converter = send_conn.base.world_manager.entity_converter_mut(&*gwm);
            let response = MessageContainer::new(response_box);
            send_conn.base.message_manager.send_response(
                &self.shared.message_kinds,
                &mut converter,
                &channel_kind,
                local_response_id,
                response,
            )
        };
        if accepted {
            self.sim_handle
                .state
                .global_response_manager
                .destroy_response_id(response_id);
            ResponseSendOutcome::Sent
        } else {
            ResponseSendOutcome::Backpressured
        }
    }

    /// Polls for a response to a previously sent request; returns `None` if not yet received.
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        let request_id = response_key.request_id();
        let (user_key, container) = self
            .sim_handle
            .state
            .global_request_manager
            .destroy_request_id(&request_id)?;
        let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
            .ok()
            .map(|boxed_s| *boxed_s)
            .unwrap();
        Some((user_key, response))
    }
    /// Drains and returns all tick-buffered messages sent by clients for the given tick.
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        let mut tick_buffer_messages = TickBufferMessages::new();
        for (_user_address, recv_conn) in self.recv.state.recv_user_connections.iter_mut() {
            // receive messages from anyone
            recv_conn.tick_buffer_messages(tick, &mut tick_buffer_messages);
        }
        tick_buffer_messages
    }

    // Updates

    /// Returns every `(room, user, entity)` tuple that currently exists —
    /// i.e. every entity in a room, crossed with every user in that room.
    /// Returns only `(room, user, entity)` tuples added since the last call to
    /// `mark_scope_checks_pending_handled()`. After initial entity/user load
    /// the returned Vec is empty every tick — zero allocation, zero iteration.
    ///
    /// Use this for incremental scope evaluation ("add every new entity once").
    /// Call `mark_scope_checks_pending_handled()` after processing each batch.
    ///
    /// For a full re-evaluation of all current pairs (e.g. at startup, or after
    /// a bulk teleport), call `mark_all_scope_checks_pending()` first to
    /// enqueue the full cross-product into the pending queue.
    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.send.state.scope_checks_cache.pending_slice().to_vec()
    }

    /// Clears the pending queue. Call after processing `scope_checks_pending()`.
    pub fn mark_scope_checks_pending_handled(&mut self) {
        self.send.state.scope_checks_cache.mark_pending_handled();
    }

    /// Re-enqueues all current (room, user, entity) tuples into the pending
    /// queue. Use this to force a full scope re-evaluation (e.g. at server
    /// startup, or after bulk world changes) without bypassing the incremental
    /// system. Follow with `scope_checks_pending()` + `mark_scope_checks_pending_handled()`.
    pub fn mark_all_scope_checks_pending(&mut self) {
        self.send.state.scope_checks_cache.mark_all_pending();
    }

    /// Slow-path recompute — used by tests to verify the cache stays
    /// in sync with `(rooms × users × entities)` truth.
    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    ///
    /// # Pipeline-mode extraction (deferred — step 4-F.naia.h)
    ///
    /// The body has three logical phases:
    ///   1. **`run_send_preamble`** — drains coordination-side handoff queues
    ///      (`scope_change_queue`, `pending_auth_grants`,
    ///      `pending_send_state_updates`), publishes
    ///      `global_priority_mirror`. Touches `self.sim_handle.state.*` and
    ///      `self.shared.*` — must stay on the coordinator.
    ///   2. **Iris three-phase loop** — Phase 1+2 dirty scan +
    ///      Phase 3 per-user pack. Touches only `self.send.state.*` and
    ///      `self.shared.*` (read-only). Safe to move to `SendState`.
    ///   3. **`flush_pending_auth_grants`** — tail step that wires
    ///      `pending_auth_grants` into per-user send-side state.
    ///      Touches `self.send.state.*` only.
    ///
    /// Step 4-F.naia.h factors phase 1 into a coordination-side method
    /// (`InternalWorldServer::run_send_preamble`) and moves phases 2 + 3 to
    /// `SendState::send_all_packets` so `SendHandle` can drive them
    /// from a background thread. See
    /// `pipeline_recv_send_independent.rs:20-31` for the deferred-work
    /// annotation and the architectural-reality block in
    /// `cyberlith/_AGENTS/MISSION_CAPACITY_UPLIFT.md` (search
    /// "4-F.cyberlith.e — multi-thread pipeline coordinator").
    ///
    /// The RTT cross-half access (`collect_outgoing_messages` reads
    /// `ping_manager.rtt_millis()` on the recv side) is resolved by
    /// mirroring into the `ConnectionShared::rtt_avg_ms` atomic that
    /// already exists post-4-C.1; verify the mirror is in sync before
    /// landing 4-F.naia.h.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        // 4-F.naia.h: send-half body lives on `SendState`. The serial /
        // cyberlith.d coordinator entry point runs the coordination-stage
        // preamble inline, then prepares + transmits synchronously.
        let plan = self.prepare_send_job(&world);
        self.transmit_send_job(world, plan);
    }

    /// MISSION_PIPELINE_API_BOUNDARY G7: a [`crate::pipeline_actors::SendStateView`]
    /// backed by this server's shared state. Used (e.g. by the determinism /
    /// byte-identity harness) to drive the **core** registry-free snapshot
    /// assembler (`SendStateView::build_needed_snapshot`) against a
    /// `WorldRefType`, exactly as the pipelined `send` bracket does.
    pub fn send_state_view(&self) -> crate::pipeline_actors::SendStateView<E> {
        crate::pipeline_actors::SendStateView::from_shared(Arc::clone(&self.shared))
    }

    /// MISSION_TICK_FLOOR Lever 3 — PREPARE half (coordination + per-user plan).
    /// Runs the coordination-stage preamble (`run_send_preamble`) and then
    /// `SendState::prepare_send_job`. On the active path the cyberlith pipeline
    /// runs this on MAIN inside the park window (before the send worker transmits
    /// the resulting plan a tick later).
    pub fn prepare_send_job<W: WorldRefType<E> + Sync>(&mut self, world: &W) -> SendPlan {
        // Defense-in-depth: drainer is also called by every InternalWorldServer::room_*
        // method, so the queue should be empty here for in-process callers.
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
        self.run_send_preamble(world);
        self.send.state.prepare_send_job(world)
    }

    /// MISSION_TICK_FLOOR Lever 3 — TRANSMIT half. Forwards to
    /// [`SendState::transmit_send_job`].
    pub fn transmit_send_job<W: WorldRefType<E> + Sync>(&mut self, world: W, plan: SendPlan) {
        self.send.state.transmit_send_job(world, plan);
    }

    /// L3 send-state seam Step 5 — drain the ACK channel on the send side
    /// (worker-preamble equivalent). Forwards to [`SendState::drain_all_acks`].
    /// `send_all_packets` calls this internally; callers that drive the lagged
    /// transmit directly (the active send worker, the harness `transmit_and_pump`)
    /// call it before `transmit_send_job`.
    pub fn drain_all_acks(&mut self) {
        self.send.state.drain_all_acks();
    }

    /// MISSION_TICK_FLOOR Lever 3 (test/diagnostic): capture a frozen snapshot
    /// of the current `global_dirty` for use as a lagged send job's plan.
    pub fn freeze_global_dirty(&self) -> FrozenGlobalDirty {
        self.shared.global_dirty.freeze()
    }

    /// 4-F.naia.b: coordinator-side preamble for the send phase. Runs
    /// `publish global_priority` + `update_entity_scopes` + `flush_pending_auth_grants`.
    /// Touches `sim_handle.*` freely. Today this is called inline from
    /// `send_all_packets`; the 4-F.cyberlith.e coordinator will call this
    /// explicitly before kicking the send thread (so `SendHandle::send_all_packets`
    /// on the send thread becomes a `self.send.state.*` + `self.shared.*`-only method).
    pub(crate) fn run_send_preamble<W: WorldRefType<E> + Sync>(&mut self, world: &W) {
        // 4-E.2e: publish-on-read for global_priority. The borrow API
        // writes go into `sim_handle.global_priority_mirror`; Iris below reads
        // from `send.global_priority`. Cost is O(N entities-with-overrides)
        // — typically 0 unless the game has actively tuned priorities.
        // This bulk clone is the single, mutation-source-agnostic priority
        // mechanism: a per-entity incremental push was considered and rejected
        // (break-even-to-slower at realistic churn + a per-mutation enqueue
        // obligation the byte-exact moat can't afford to miss).
        self.send
            .state
            .global_priority
            .clone_from(&self.sim_handle.state.global_priority_mirror);

        // update entity scopes
        #[cfg(feature = "bench_instrumentation")]
        let _scope_t0 = std::time::Instant::now();
        self.update_entity_scopes(world);
        #[cfg(feature = "bench_instrumentation")]
        bench_scope_counters::NS_UPDATE_ENTITY_SCOPES.fetch_add(
            _scope_t0.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        // Flush deferred auth grants before Iris reads send.send_user_connections.
        // Auth grants mutate send_conn.base.world_manager via host_send_set_auth,
        // so they must run before the Iris build phase to be packed THIS tick.
        self.flush_pending_auth_grants();
    }

    /// 4-F.naia.b: drains `shared.pending_auth_grants` and applies the
    /// SetAuthority messages onto `send_user_connections`. Coord-stage —
    /// runs as part of `run_send_preamble`. Lock order: takes the
    /// `pending_auth_grants` Mutex (position #7, last) briefly.
    pub(crate) fn flush_pending_auth_grants(&mut self) {
        let pending_grants = std::mem::take(&mut *self.shared.pending_auth_grants.lock());
        for (owner_user_key, global_entity, _granted_status) in pending_grants {
            // Collect addresses first to avoid borrowing issues
            let user_addresses: Vec<SocketAddr> = self
                .send
                .state
                .send_user_connections
                .keys()
                .copied()
                .collect();
            // Send SetAuthority to all users in scope (canonical path)
            for address in user_addresses {
                let Some(send_conn) = self.send.state.send_user_connections.get_mut(&address)
                else {
                    continue;
                };
                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(&global_entity)
                {
                    continue;
                }
                let user_key_for_conn = send_conn.user_key;
                let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
                if owner_user_key == user_key_for_conn {
                    new_status = EntityAuthStatus::Granted;
                }
                // Use host_send_set_auth which handles both HostEntity and RemoteEntity
                send_conn
                    .base
                    .world_manager
                    .host_send_set_auth(&global_entity, new_status);
                #[cfg(feature = "e2e_debug")]
                if new_status == EntityAuthStatus::Granted {
                    SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                    SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, mut world: W) -> EntityMut<'_, E, W> {
        let world_entity = world.spawn_entity();

        self.spawn_entity_inner(&world_entity);

        EntityMut::new(self, world, &world_entity)
    }

    /// Creates a new Entity with a specific id
    fn spawn_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self
            .shared
            .global_entity_map
            .write()
            .spawn(*world_entity, None);
        let idx = self
            .shared
            .global_world_manager
            .write()
            .insert_entity_record(&global_entity, EntityOwner::Server);
        if idx.is_valid() {
            self.shared.idx_to_world.write()[idx.as_usize()] = Some(*world_entity);
        }
    }

    fn spawn_static_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self
            .shared
            .global_entity_map
            .write()
            .spawn(*world_entity, None);
        let idx = self
            .shared
            .global_world_manager
            .write()
            .insert_static_entity_record(&global_entity, EntityOwner::Server);
        if idx.is_valid() {
            self.shared.idx_to_world.write()[idx.as_usize()] = Some(*world_entity);
        }
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.spawn_entity_inner(entity);
    }

    /// Bevy adapter crates only: register an already-spawned Bevy entity as a
    /// static (immutable) naia entity. Static entities are never diff-tracked
    /// after initial replication. Post-spawn mutation panics via EntityMut.
    pub fn enable_static_entity_replication(&mut self, entity: &E) {
        self.spawn_static_entity_inner(entity);
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(world_entity);
    }

    /// Pauses replication for this entity: component changes are no longer
    /// transmitted to any client until `resume_entity_replication` is called.
    /// The entity remains spawned on clients; it simply stops receiving updates.
    ///
    /// # Adapter use only
    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            warn!("pause_entity_replication: entity not found in global map");
            return;
        };
        self.shared
            .global_world_manager
            .write()
            .pause_entity_replication(&global_entity);
    }

    /// Resumes replication for an entity previously paused with
    /// `pause_entity_replication`. Component changes will again be tracked and
    /// transmitted to clients on the next send tick.
    ///
    /// # Adapter use only
    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            warn!("resume_entity_replication: entity not found in global map");
            return;
        };
        self.shared
            .global_world_manager
            .write()
            .resume_entity_replication(&global_entity);
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        self.shared
            .global_entity_map
            .write()
            .set_global_entity_counter_for_test(value);
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn inject_tick_buffer_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        host_tick: &Tick,
        message_tick: &Tick,
        message: &M,
    ) -> bool {
        let channel_kind = ChannelKind::of::<C>();
        let message_box = M::clone_box(message);
        let container = MessageContainer::new(message_box);
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            warn!(
                "inject_tick_buffer_message: user {:?} does not exist",
                user_key
            );
            return false;
        };
        let address = user.address();
        let Some(recv_conn) = self.recv.state.recv_user_connections.get_mut(&address) else {
            warn!(
                "inject_tick_buffer_message: no connection for user {:?}",
                user_key
            );
            return false;
        };
        recv_conn.inject_tick_buffer_message(&channel_kind, host_tick, message_tick, container)
    }

    /// Returns `true` if the entity has been marked as static (never re-sent after initial spawn).
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        // G-unify 2b-2: delegate to the canonical CoordHandle body.
        self.sim_handle.entity_is_static(world_entity)
    }

    /// Marks an entity as static; its component data will not be re-sent after the initial spawn packet.
    pub fn mark_entity_as_static(&mut self, world_entity: &E) {
        // G-unify 2b-2: delegate to the canonical CoordHandle body (byte-identical).
        self.sim_handle.mark_entity_as_static(world_entity);
    }

    /// Returns `true` if the entity is currently in `Delegated` replication mode.
    pub fn entity_is_delegated(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return false;
        };
        self.shared
            .global_world_manager
            .read()
            .entity_is_delegated(&global_entity)
    }

    // ========================================================================
    // Replicated Resources
    // ========================================================================
    //
    // A Replicated Resource is internally a hidden 1-component entity that:
    //   - Is registered in the per-world `ResourceRegistry` keyed by `R`'s
    //     TypeId, allowing O(1) `resource_entity::<R>()` lookups.
    //   - Is auto-included in every connected user's scope (so resources
    //     reach every client without explicit room/scope work).
    //   - Otherwise reuses the existing entity replication pipeline 100%
    //     (spawn/update/despawn, per-field diff tracking, authority).
    //
    // See `_AGENTS/RESOURCES_PLAN.md`.

    /// Insert a Replicated Resource using a dynamic entity ID.
    ///
    /// Spawns the hidden entity, attaches `value` as its sole replicated
    /// component, registers it in the per-world `ResourceRegistry`, and
    /// auto-includes it in every currently-connected user's scope.
    ///
    /// Returns the underlying world-entity handle for tests / advanced use.
    /// Bevy adapter callers will not usually surface this entity to user
    /// code (resources are entity-less from the user's POV).
    ///
    /// Errors with `ResourceAlreadyExists` if `R` was already inserted
    /// in this world. The world remains unchanged on error.
    /// Insert a Replicated Resource.
    ///
    /// Pass `is_static = true` for long-lived singletons that never change
    /// after insertion (no diff-tracking on the wire). Pass `false` for
    /// resources whose fields are updated over time (delta-tracked).
    ///
    /// Errors with `ResourceAlreadyExists` if `R` was already inserted.
    /// The world remains unchanged on error.
    pub fn insert_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        mut world: W,
        value: R,
        is_static: bool,
    ) -> Result<E, ResourceAlreadyExists> {
        let world_entity = world.spawn_entity();
        if is_static {
            self.spawn_static_entity_inner(&world_entity);
        } else {
            self.spawn_entity_inner(&world_entity);
        }
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(&world_entity)
            .expect("entity just spawned must be in global map");

        if let Err(e) = self
            .sim_handle
            .state
            .resource_registry
            .insert::<R>(global_entity)
        {
            self.despawn_entity_worldless(&world_entity);
            world.despawn_entity(&world_entity);
            return Err(e);
        }

        self.insert_component(&mut world, &world_entity, value);

        let user_keys: Vec<UserKey> = self.sim_handle.state.user_store.keys_copied();
        for user_key in user_keys {
            self.user_scope_set_entity(&user_key, &world_entity, true);
        }

        Ok(world_entity)
    }

    /// Remove the resource of type `R` if present. Despawns the hidden
    /// entity (which propagates a despawn to every client where it was
    /// in scope) and clears the registry entries on both sides.
    ///
    /// Returns `true` if a resource was removed, `false` if `R` was not
    /// present.
    pub fn remove_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        mut world: W,
    ) -> bool {
        let Some(global_entity) = self.sim_handle.state.resource_registry.remove::<R>() else {
            return false;
        };
        let world_entity = match self
            .shared
            .global_entity_map
            .read()
            .global_entity_to_entity(&global_entity)
        {
            Ok(e) => e,
            Err(_) => return true, // registry stale; nothing more to do
        };
        // Despawn from inner tracking (scope, priority, replication state)
        self.despawn_entity_worldless(&world_entity);
        // Then despawn from the world itself.
        world.despawn_entity(&world_entity);
        true
    }

    /// O(1): the hidden entity carrying resource `R`, or `None` if
    /// `R` is not currently inserted.
    pub fn resource_entity<R: ReplicatedComponent>(&self) -> Option<E> {
        let global_entity = self.sim_handle.state.resource_registry.entity_for::<R>()?;
        self.shared
            .global_entity_map
            .read()
            .global_entity_to_entity(&global_entity)
            .ok()
    }

    /// O(1): is `world_entity` a hidden resource entity?
    /// Used by Bevy adapter event-emission filter (D13) to suppress
    /// SpawnEntityEvent / component events for resource entities.
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        // G-unify 2b-2: delegate to the canonical CoordHandle body.
        self.sim_handle.is_resource_entity(world_entity)
    }

    /// True iff a resource of type `R` is currently inserted.
    pub fn has_resource<R: ReplicatedComponent>(&self) -> bool {
        self.sim_handle
            .state
            .resource_registry
            .entity_for::<R>()
            .is_some()
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        self.sim_handle.state.resource_registry.len()
    }

    /// Read-only handle to the per-resource priority state.
    /// Returns `None` if the resource is not currently inserted.
    /// Per D9 / §4.4 of RESOURCES_PLAN: per-resource priority is just
    /// per-entity priority on the hidden resource entity. Default gain
    /// is 1.0 (same as any entity); no special "Resource" priority tier.
    pub fn resource_priority<R: ReplicatedComponent>(&self) -> Option<EntityPriorityRef<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority(entity))
    }

    /// Mutable handle to the per-resource priority state.
    /// Returns `None` if the resource is not currently inserted.
    /// User can call `.set_gain(f32)` to tune priority or `.boost_once(f32)`
    /// for a one-shot bump.
    pub fn resource_priority_mut<R: ReplicatedComponent>(
        &mut self,
    ) -> Option<EntityPriorityMut<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority_mut(entity))
    }

    /// Server-side authority status for resource `R`. Returns `None`
    /// if `R` is not currently inserted or if the resource is not
    /// configured for delegation.
    pub fn resource_authority_status<R: ReplicatedComponent>(&self) -> Option<EntityAuthStatus> {
        let entity = self.resource_entity::<R>()?;
        self.entity_authority_status(&entity)
    }

    /// Iterate over the hidden entities of all currently-inserted resources.
    /// Used by the connect-flow to auto-include all resources in a new
    /// user's scope.
    pub fn resource_entities(&self) -> Vec<E> {
        let mut out = Vec::with_capacity(self.sim_handle.state.resource_registry.len());
        for global_entity in self.sim_handle.state.resource_registry.entities() {
            if let Ok(e) = self
                .shared
                .global_entity_map
                .read()
                .global_entity_to_entity(global_entity)
            {
                out.push(e);
            }
        }
        out
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.shared
            .global_world_manager
            .read()
            .entity_replication_config(&global_entity)
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .shared
            .global_world_manager
            .write()
            .server_take_authority(&global_entity);

        if let Ok(previous_owner) = result {
            // When server takes authority, send Denied to clients whose state will change:
            // - If there was a client holder (Granted→Denied): send only to that client
            // - If no holder (Available→Denied): send to all clients in scope
            self.send_take_authority_messages(&global_entity, previous_owner);
            self.recv
                .state
                .incoming_world_events
                .push_auth_reset(world_entity);
        }
        result.map(|_| ())
    }

    fn send_take_authority_messages(
        &mut self,
        global_entity: &GlobalEntity,
        previous_owner: AuthOwner,
    ) {
        // Server has taken authority - send appropriate messages based on previous state
        match previous_owner {
            AuthOwner::Client(prev_holder_key) => {
                // There was a client holder - only they need to transition (Granted→Denied)
                // Other clients were already Denied, no message needed
                if let Some(user) = self.sim_handle.state.user_store.get(&prev_holder_key) {
                    if let Some(send_conn) = self
                        .send
                        .state
                        .send_user_connections
                        .get_mut(&user.address())
                    {
                        if send_conn
                            .base
                            .world_manager
                            .has_global_entity(global_entity)
                        {
                            send_conn
                                .base
                                .world_manager
                                .host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                        }
                    }
                }
            }
            AuthOwner::None => {
                // No holder - all clients were Available, all need to transition to Denied
                for (_user_key, user) in self.sim_handle.state.user_store.iter() {
                    if let Some(send_conn) = self
                        .send
                        .state
                        .send_user_connections
                        .get_mut(&user.address())
                    {
                        if !send_conn
                            .base
                            .world_manager
                            .has_global_entity(global_entity)
                        {
                            continue;
                        }
                        send_conn
                            .base
                            .world_manager
                            .host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                    }
                }
            }
            AuthOwner::Server => {
                // Server already had authority - no change needed
            }
        }
    }

    fn send_reset_authority_messages(&mut self, global_entity: &GlobalEntity) {
        // authority was released from entity
        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_user_key, user) in self.sim_handle.state.user_store.iter() {
            if let Some(send_conn) = self
                .send
                .state
                .send_user_connections
                .get_mut(&user.address())
            {
                // Check if entity exists on the client (as either HostEntity or RemoteEntity)
                // After migration, the entity is a RemoteEntity on the client, but the server
                // still sends from HostEntity perspective and the client's routing handles it
                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not mapped to this connection
                    continue;
                }

                // Send UpdateAuthority action through EntityActionEvent system
                // The server always sends from HostEntity perspective, and the client's
                // routing logic will handle converting it to the correct entity type
                send_conn
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, EntityAuthStatus::Available);
            }
        }
    }

    /// Applies a new [`ReplicationConfig`] to an entity, changing its visibility and authority model.
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self
            .shared
            .global_world_manager
            .read()
            .has_entity(&global_entity)
        {
            panic!("Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Server, before configuring replication.");
        }
        let entity_owner = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned: bool = entity_owner.is_server();
        let client_owned: bool = entity_owner.is_client();
        // When the server initiates delegation on a client-owned entity
        // (per spec [entity-ownership-11]), `entity_enable_delegation` needs
        // the owning client's key as `client_origin` so the migration flow
        // runs (`enable_delegation_client_owned_entity`) AND so the owning
        // client doesn't receive an EnableDelegation message it can't
        // route — its `HostEntityChannel::process_messages` would panic
        // with "unexpected message type: EnableDelegation".
        let client_origin: Option<UserKey> = match entity_owner {
            EntityOwner::Client(uk)
            | EntityOwner::ClientPublic(uk)
            | EntityOwner::ClientWaiting(uk) => Some(uk),
            EntityOwner::Server | EntityOwner::Local => None,
        };
        let prev_config = self
            .shared
            .global_world_manager
            .read()
            .entity_replication_config(&global_entity)
            .unwrap();
        if prev_config == config {
            // Fully identical — no-op
            return;
        }

        // Handle publicity state machine only when publicity changed
        if prev_config.publicity != config.publicity {
            match prev_config.publicity {
                Publicity::Private => {
                    if server_owned {
                        panic!("Server-owned entity should never be private");
                    }
                    match config.publicity {
                        Publicity::Private => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                        Publicity::Public => {
                            // private -> public
                            self.publish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Delegated => {
                            // private -> delegated
                            // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                            // which transfers ownership to server
                            self.publish_entity(world, &global_entity, world_entity, true);
                            self.entity_enable_delegation(
                                world,
                                &global_entity,
                                world_entity,
                                client_origin,
                            );
                        }
                    }
                }
                Publicity::Public => {
                    match config.publicity {
                        Publicity::Private => {
                            // public -> private
                            if server_owned {
                                panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                            }
                            self.unpublish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Public => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                        Publicity::Delegated => {
                            // public -> delegated
                            // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                            // which transfers ownership to server
                            self.entity_enable_delegation(
                                world,
                                &global_entity,
                                world_entity,
                                client_origin,
                            );
                        }
                    }
                }
                Publicity::Delegated => {
                    if client_owned {
                        panic!("Client-owned entity should never be delegated");
                    }
                    match config.publicity {
                        Publicity::Private => {
                            // delegated -> private
                            if server_owned {
                                panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                            }
                            self.entity_disable_delegation(world, &global_entity, world_entity);
                            self.unpublish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Public => {
                            // delegated -> public
                            self.entity_disable_delegation(world, &global_entity, world_entity);
                        }
                        Publicity::Delegated => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                    }
                }
            }
        }

        // Always persist the scope_exit field regardless of whether publicity changed
        self.shared
            .global_world_manager
            .write()
            .entity_set_scope_exit(&global_entity, config.scope_exit);
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_give_authority(
        &mut self,
        origin_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per contract [entity-authority-12] ("server give_authority
        // requires scope"): the target user must be able to see the
        // entity, otherwise return `NotInScope` and leave the holder
        // unchanged. Without this gate the server could silently grant
        // authority to an out-of-scope user, who would never receive the
        // matching SetAuthority message and would diverge from server
        // state.
        if !self.user_scope_has_entity(origin_user, world_entity) {
            return Err(AuthorityError::NotInScope);
        }

        // Use the server-priority give path so we override any current
        // holder (per contract [entity-authority-10]). The previous
        // `client_request_authority` path failed with NotAvailable
        // whenever the entity was already held — including by the same
        // user — which broke the "server give overrides current holder"
        // contract.
        let previous_owner = self
            .shared
            .global_world_manager
            .write()
            .server_give_authority_to_client(&global_entity, origin_user)?;

        // Idempotent re-give to the same user: the auth-handler already
        // returned without state change (see
        // `server_give_authority_to_client`); skip fan-out so we don't
        // drive an illegal Granted→Granted transition through the
        // per-client auth channel.
        if previous_owner == AuthOwner::Client(*origin_user) {
            return Ok(());
        }

        // entity authority was granted for origin user
        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (user_key, user) in self.sim_handle.state.user_store.iter() {
            let Some(send_conn) = self
                .send
                .state
                .send_user_connections
                .get_mut(&user.address())
            else {
                continue;
            };
            // Check if entity exists on the client (as either HostEntity or RemoteEntity)
            // After migration, the entity is a RemoteEntity on the client, but the server
            // still sends from HostEntity perspective and the client's routing handles it
            if !send_conn
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                // entity is not mapped to this connection
                continue;
            }

            let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
            if origin_user == user_key {
                new_status = EntityAuthStatus::Granted;
            }

            // Send UpdateAuthority action through EntityActionEvent system
            // The server always sends from HostEntity perspective, and the client's
            // routing logic will handle converting it to the correct entity type
            send_conn
                .base
                .world_manager
                .host_send_set_auth(&global_entity, new_status);
            #[cfg(feature = "e2e_debug")]
            if new_status == EntityAuthStatus::Granted {
                SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
            }
        }

        // SetAuthority is sent in the per-connection loop above — do NOT also push to
        // auth_grants, which would queue a second SetAuthority send and drive illegal
        // transitions (e.g. Granted→Denied for the grantee, or Denied→Denied for observers).
        // Covered by [entity-authority-17] @Scenario(38).

        // Push to events for external systems (e.g., Bevy adapter, test harness)
        // Events are separate from network messages - they're notifications for external consumers
        self.recv
            .state
            .incoming_world_events
            .push_auth_grant(origin_user, world_entity);

        Ok(())
    }

    fn entity_handle_client_request_authority(
        &mut self,
        requester_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        if !self.user_scope_has_entity(requester_user, world_entity) {
            return Err(AuthorityError::NotInScope);
        }

        let requester = AuthOwner::from_user_key(Some(requester_user));
        self.shared
            .global_world_manager
            .write()
            .client_request_authority(&global_entity, &requester)?;

        for (user_key, user) in self.sim_handle.state.user_store.iter() {
            let Some(send_conn) = self
                .send
                .state
                .send_user_connections
                .get_mut(&user.address())
            else {
                continue;
            };
            if !send_conn
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                continue;
            }
            let new_status = if requester_user == user_key {
                EntityAuthStatus::Granted
            } else {
                EntityAuthStatus::Denied
            };
            send_conn
                .base
                .world_manager
                .host_send_set_auth(&global_entity, new_status);
        }

        self.recv
            .state
            .incoming_world_events
            .push_auth_grant(requester_user, world_entity);

        Ok(())
    }

    /// Sends `SetAuthority(Denied)` to a single user, resolving that user's
    /// pending `Requested` state after a refused authority request.
    fn notify_user_auth_denied(&mut self, user_key: &UserKey, global_entity: &GlobalEntity) {
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            return;
        };
        let address = user.address();
        let Some(send_conn) = self.send.state.send_user_connections.get_mut(&address) else {
            return;
        };
        if !send_conn
            .base
            .world_manager
            .has_global_entity(global_entity)
        {
            return;
        }
        send_conn
            .base
            .world_manager
            .host_send_set_auth(global_entity, EntityAuthStatus::Denied);
    }

    fn entity_enable_delegation_response(
        &mut self,
        _user_key: &UserKey,
        _global_entity: &GlobalEntity,
    ) {
        // EnableDelegationResponse does NOT send SetAuthority messages.
        // Enabling delegation establishes the delegated-mode baseline as Available (AuthNone) for clients.
        // Any Denied/Granted status changes come ONLY from subsequent authority operations (request/give/take/release).
        // The client initializes local auth status to Available when processing EnableDelegation message,
        // so no SetAuthority message is needed here.
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        // G-unify 2b-2: delegate to the canonical CoordHandle body.
        self.sim_handle.entity_authority_status(world_entity)
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let releaser = AuthOwner::from_user_key(origin_user);
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .shared
            .global_world_manager
            .write()
            .client_release_authority(&global_entity, &releaser);
        if result.is_ok() {
            self.send_reset_authority_messages(&global_entity);
        }
        result
    }

    /// Enable delegation for a server-owned entity
    ///
    /// This enables delegation for the given entity, allowing authority to be
    /// requested/released. The entity must be server-owned and Public.
    /// Returns true if delegation was enabled, false otherwise.
    pub(crate) fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        let global_entity = match self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        {
            Ok(ge) => ge,
            Err(_) => return false,
        };

        // Only enable delegation for server-owned entities
        let owner = self.entity_owner(world_entity);
        if !owner.is_server() {
            return false;
        }

        self.entity_enable_delegation(world, &global_entity, world_entity, None);
        true
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<W: WorldMutType<E>>(
        &'_ mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'_, E, W> {
        if world.has_entity(entity) {
            return EntityMut::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        world.entities()
    }

    // This intended to be used by adapter crates
    pub(crate) fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        // G-unify 2b-2: delegate to the canonical CoordHandle body. This also
        // resolves a latent drift — the prior resident body `.unwrap()`-panicked
        // on an unknown entity where the CoordHandle (pipelined) body returns
        // `EntityOwner::Local`; both now share the safe `Local` fallback.
        self.sim_handle.entity_owner(world_entity)
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.sim_handle.user_exists(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    ///
    /// # Panics
    /// Panics if no user exists for the given key. Prefer [`user_opt`](Self::user_opt)
    /// when calling from a context where the key may be stale (e.g., inside a
    /// disconnect handler that received a copy of the key before disconnect was processed).
    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, E> {
        if self.sim_handle.state.user_store.contains(user_key) {
            return UserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns `Some(UserRef)` if the user exists, or `None` if the key is stale.
    ///
    /// Use this instead of [`user`](Self::user) when you cannot guarantee the key is still live.
    pub fn user_opt(&'_ self, user_key: &UserKey) -> Option<UserRef<'_, E>> {
        if self.sim_handle.state.user_store.contains(user_key) {
            Some(UserRef::new(self, user_key))
        } else {
            None
        }
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    ///
    /// # Panics
    /// Panics if no user exists for the given key. Prefer [`user_mut_opt`](Self::user_mut_opt)
    /// when calling from a context where the key may be stale.
    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, E> {
        if self.sim_handle.state.user_store.contains(user_key) {
            return UserMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns `Some(UserMut)` if the user exists, or `None` if the key is stale.
    ///
    /// Use this instead of [`user_mut`](Self::user_mut) when you cannot guarantee the key is still live.
    pub fn user_mut_opt(&'_ mut self, user_key: &UserKey) -> Option<UserMut<'_, E>> {
        if self.sim_handle.state.user_store.contains(user_key) {
            Some(UserMut::new(self, user_key))
        } else {
            None
        }
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, user) in self.sim_handle.state.user_store.iter() {
            if self
                .send
                .state
                .send_user_connections
                .contains_key(&user.address())
            {
                output.push(*user_key);
            }
        }

        output
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.sim_handle.state.user_store.len()
    }

    /// Returns the number of users that have fully connected (handshake complete).
    pub fn user_count(&self) -> usize {
        self.user_keys().len()
    }

    /// Returns the total number of replicated entities currently tracked by the server.
    pub fn entity_count(&self) -> usize {
        self.shared.global_entity_map.read().entity_count()
    }

    /// Returns a UserScopeRef, which is used to query whether a given user has
    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        if self.sim_handle.state.user_store.contains(user_key) {
            return UserScopeRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        if self.sim_handle.state.user_store.contains(user_key) {
            return UserScopeMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    // Priority

    /// Read-only handle to the sender-wide (global) priority state for `entity`.
    /// Combined multiplicatively with the per-user gain at sort time.
    ///
    /// 4-E.2e: read target is `sim_handle.global_priority_mirror`. The send
    /// thread's read target (`send.global_priority`) is a clone refreshed
    /// at the top of `send_all_packets`.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        self.sim_handle.state.global_priority_mirror.get_ref(entity)
    }

    /// Mutable handle to the sender-wide (global) priority state for `entity`.
    /// Lazy-creates an entry on first write.
    ///
    /// 4-E.2e: writes target `sim_handle.global_priority_mirror`. The publish
    /// step at the top of the next `send_all_packets` carries the change
    /// over to `send.global_priority` via the wholesale publish-on-read clone.
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        self.sim_handle.state.global_priority_mirror.get_mut(entity)
    }

    /// Read-only handle to the per-user priority state for `entity` on the
    /// given user's connection. Evicted on scope exit for that user.
    pub fn user_entity_priority(&self, user_key: &UserKey, entity: E) -> EntityPriorityRef<'_, E> {
        // Fetch this user's layer; if none exists yet, fall back to the
        // global `Ref`-on-missing semantics via a fresh empty layer.
        // Safe because `EntityPriorityRef` reads `Option<&EntityPriorityData>`
        // via the state map — no allocation is required on the read path.
        match self.send.state.user_priorities.get(user_key) {
            Some(layer) => layer.get_ref(entity),
            None => {
                // No entry exists for this user; return an empty ref by
                // peeking through an ephemeral empty layer. We use a static
                // path via a constructor that reads None for `state`.
                EntityPriorityRef::empty(entity)
            }
        }
    }

    /// Mutable handle to the per-user priority state for `entity` on the given
    /// user's connection. Lazy-creates the user's priority layer and the entity
    /// entry on first write.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        let layer = self
            .send
            .state
            .user_priorities
            .entry(*user_key)
            .or_default();
        layer.get_mut(entity)
    }

    // Ticks

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        self.sim_handle.current_tick()
    }

    /// Gets the current average tick duration of the Server
    pub fn average_tick_duration(&self) -> Duration {
        self.shared.time_manager.read().average_tick_duration()
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn create_room(&'_ mut self) -> RoomMut<'_, E> {
        let new_room = Room::new();
        let room_key = self.sim_handle.state.room_store.insert(new_room);
        RoomMut::new(self, &room_key)
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.sim_handle.state.room_store.contains(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, E> {
        if self.sim_handle.state.room_store.contains(room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        if self.sim_handle.state.room_store.contains(room_key) {
            return RoomMut::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Return a list of all the Server's Rooms' keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.sim_handle.state.room_store.keys()
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.sim_handle.state.room_store.len()
    }

    /// Returns the total number of rooms that currently exist.
    pub fn room_count(&self) -> usize {
        self.room_keys().len()
    }

    // Bandwidth monitoring
    /// Total outgoing bandwidth averaged over the monitor window (bytes/sec).
    pub fn outgoing_bandwidth_total(&self) -> f32 {
        self.send.state.send_io.outgoing_bandwidth_total()
    }

    /// Bytes sent (post-compression, pre-transport) during the most recent
    /// `send_all_packets` call. Precise, non-rolling counter. Read after a
    /// tick has run; reset to 0 at the start of the next `send_all_packets`.
    pub fn outgoing_bytes_last_tick(&self) -> u64 {
        self.send.state.send_io.outgoing_bytes_last_tick()
    }

    /// Total incoming bandwidth averaged over the monitor window (bytes/sec).
    pub fn incoming_bandwidth_total(&self) -> f32 {
        self.recv.state.recv_io.incoming_bandwidth_total()
    }

    /// Outgoing bandwidth to a specific client address, averaged over the monitor window (bytes/sec).
    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        self.send
            .state
            .send_io
            .outgoing_bandwidth_to_client(address)
    }

    /// Incoming bandwidth from a specific client address, averaged over the monitor window (bytes/sec).
    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        self.recv
            .state
            .recv_io
            .incoming_bandwidth_from_client(address)
    }

    // Ping
    /// Gets the average Round Trip Time measured to the given User's Client
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.sim_handle.state.user_store.get(user_key) {
            if let Some(recv_conn) = self.recv.state.recv_user_connections.get(&user.address()) {
                return Some(recv_conn.ping_manager.rtt_average);
            }
        }
        None
    }

    /// Gets the average Jitter measured in connection to the given User's
    /// Client
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.sim_handle.state.user_store.get(user_key) {
            if let Some(recv_conn) = self.recv.state.recv_user_connections.get(&user.address()) {
                return Some(recv_conn.ping_manager.jitter_average);
            }
        }
        None
    }

    // Historian — lag-compensation snapshot buffer

    /// Enable the per-tick snapshot buffer for server-side lag compensation.
    ///
    /// `max_ticks` controls how many past ticks are retained. A value of 64
    /// covers ~3 seconds at 20 Hz, which is appropriate for most games.
    /// Call once at startup; calling again replaces the buffer.
    pub fn enable_historian(&mut self, max_ticks: u16) {
        self.sim_handle.state.historian = Some(crate::historian::Historian::new(max_ticks));
    }

    /// Like `enable_historian`, but only snapshots the component kinds in
    /// `filter`. Use this to reduce per-tick clone cost when you only need
    /// a subset of components for lag-compensation (e.g. `Position`, `Health`).
    pub fn enable_historian_filtered(
        &mut self,
        max_ticks: u16,
        filter: impl IntoIterator<Item = naia_shared::ComponentKind>,
    ) {
        self.sim_handle.state.historian =
            Some(crate::historian::Historian::new_filtered(max_ticks, filter));
    }

    /// Record a snapshot of all replicated component values at the given tick.
    ///
    /// Call this each tick after game-state mutation and before
    /// `send_all_packets`, so the snapshot reflects authoritative state.
    /// This is a no-op if `enable_historian()` has not been called.
    pub fn record_historian_tick<W: WorldRefType<E>>(&mut self, world: W, tick: Tick) {
        let entity_map = self.shared.global_entity_map.read();
        if let Some(historian) = &mut self.sim_handle.state.historian {
            historian.record_tick(
                tick,
                &*self.shared.global_world_manager.read(),
                &*entity_map,
                &world,
            );
        }
    }

    /// Returns a read-only reference to the Historian, or `None` if it has not
    /// been enabled via `enable_historian()`.
    pub fn historian(&self) -> Option<&crate::historian::Historian> {
        self.sim_handle.state.historian.as_ref()
    }

    /// Returns a snapshot of per-connection diagnostics for the given user.
    ///
    /// Returns `None` if the user is not connected. All fields are rolling
    /// averages or short-window estimates computed on demand; no per-tick
    /// allocation occurs.
    pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats> {
        let user = self.sim_handle.state.user_store.get(user_key)?;
        let recv_conn = self.recv.state.recv_user_connections.get(&user.address())?;
        let send_conn = self.send.state.send_user_connections.get(&user.address())?;
        let pm = &recv_conn.ping_manager;
        Some(ConnectionStats {
            rtt_ms: pm.rtt_average,
            rtt_p50_ms: pm.rtt_p50_ms(),
            rtt_p99_ms: pm.rtt_p99_ms(),
            jitter_ms: pm.jitter_average,
            packet_loss_pct: send_conn.base.packet_loss_pct(),
            kbps_sent: self
                .send
                .state
                .send_io
                .outgoing_bandwidth_to_client(&user.address()),
            kbps_recv: self
                .recv
                .state
                .recv_io
                .incoming_bandwidth_from_client(&user.address()),
        })
    }

    /// Whether the user's send-side connection has been materialized.
    ///
    /// This is a side-effect-free park-window query. It reads only the
    /// canonical `send_user_connections` membership and does not require
    /// bandwidth monitoring to be enabled.
    pub fn user_connection_ready(&self, user_key: &UserKey) -> bool {
        user_connection_ready_impl(
            &self.sim_handle.state.user_store,
            &self.send.state.send_user_connections,
            user_key,
        )
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, world_entity: &E) {
        if !world.has_entity(world_entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // Delete from world
        world.despawn_entity(world_entity);

        self.despawn_entity_worldless(world_entity);
    }

    /// Removes an entity from all replication state without touching the world (adapter use only).
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return;
        };
        // Priority layer eviction: drop global entry + every user's per-user
        // entry for this entity. Prevents leaks across entity lifetime.
        // 4-E.2e: evict from BOTH mirror (borrow-API source) and the
        // send-side authoritative copy — publish-on-read at next
        // `send_all_packets` would also clear it, but doing it eagerly
        // here keeps the two copies bit-identical between ticks.
        self.sim_handle
            .state
            .global_priority_mirror
            .on_despawn(world_entity);
        self.send.state.global_priority.on_despawn(world_entity);
        for layer in self.send.state.user_priorities.values_mut() {
            layer.on_scope_exit(world_entity);
        }
        // Drop every (*, *, world_entity) tuple from the scope-checks cache.
        // Single linear retain — covers all rooms that previously contained
        // the entity, replacing what would otherwise be one retain per
        // affected room.
        self.send
            .state
            .scope_checks_cache
            .on_entity_despawned(*world_entity);
        self.cleanup_entity_replication(&global_entity);
        self.shared
            .global_world_manager
            .write()
            .remove_entity_record(&global_entity);
        self.shared
            .global_entity_map
            .write()
            .despawn_by_global(&global_entity);
    }

    fn cleanup_entity_replication(&mut self, global_entity: &GlobalEntity) {
        self.despawn_entity_from_all_connections(global_entity);

        // Delete scope
        self.send
            .state
            .entity_scope_map
            .remove_entity(global_entity);

        // Delete room cache entry
        if let Some(room_keys) = self
            .send
            .state
            .entity_room_map
            .remove_from_all_rooms(global_entity)
        {
            for room_key in room_keys {
                if let Some(room) = self.sim_handle.state.room_store.get_mut(&room_key) {
                    room.remove_entity(global_entity, true);
                }
            }
        }

        // Remove from ECS Record
        self.shared
            .global_world_manager
            .write()
            .remove_entity_diff_handlers(global_entity);
    }

    fn despawn_entity_from_all_connections(&mut self, global_entity: &GlobalEntity) {
        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        let entity_idx = self.entity_global_idx(global_entity);
        if entity_idx.is_valid() {
            self.shared.idx_to_world.write()[entity_idx.as_usize()] = None;
        }
        for (_, send_conn) in self.send.state.send_user_connections.iter_mut() {
            if !send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                continue;
            }
            // remove entity from user connection
            send_conn.base.world_manager.despawn_entity(global_entity);
            send_conn.clear_entity_visible(entity_idx);
        }
    }

    //// Entity Scopes

    /// Remove all entities from a User's scope
    pub(crate) fn user_scope_remove_user(&mut self, user_key: &UserKey) {
        self.send.state.entity_scope_map.remove_user(user_key);
    }

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        is_contained: bool,
    ) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per [entity-authority-12]: If the authority-holding client loses scope for E,
        // the server MUST release/reset authority for E.
        // Check if user is being removed from scope and is the authority holder
        // 4-F.naia.a: drop the read guard explicitly via a let binding so it
        // cannot remain alive into the if body's write-guard acquisition
        // (parking_lot RwLock would deadlock on the upgrade).
        let is_authority_holder = self
            .shared
            .global_world_manager
            .read()
            .user_is_authority_holder(user_key, &global_entity);
        if !is_contained && is_authority_holder {
            // Release authority - the user is losing scope while holding authority
            let releaser = AuthOwner::Client(*user_key);
            if self
                .shared
                .global_world_manager
                .write()
                .client_release_authority(&global_entity, &releaser)
                .is_ok()
            {
                // Notify other clients that authority is now Available
                self.send_reset_authority_messages(&global_entity);
            }
        }

        // Per [entity-publication]: silently ignore explicit include() for Private entities
        // when the user is not the owner — mirrors the guard in user_scope_has_entity().
        if is_contained {
            let is_private = self
                .shared
                .global_world_manager
                .read()
                .entity_replication_config(&global_entity)
                .map(|c| matches!(c.publicity, Publicity::Private))
                .unwrap_or(false);
            if is_private {
                let is_owner = match self
                    .shared
                    .global_world_manager
                    .read()
                    .entity_owner(&global_entity)
                {
                    Some(
                        EntityOwner::Client(owner_key)
                        | EntityOwner::ClientWaiting(owner_key)
                        | EntityOwner::ClientPublic(owner_key),
                    ) => owner_key == *user_key,
                    _ => false,
                };
                if !is_owner {
                    return;
                }
            }
        }

        self.send
            .state
            .entity_scope_map
            .insert(*user_key, global_entity, is_contained);
        self.shared
            .scope_change_queue
            .lock()
            .push_back(ScopeChange::ScopeToggled(
                *user_key,
                global_entity,
                is_contained,
            ));
    }

    pub(crate) fn user_scope_has_entity(&self, user_key: &UserKey, world_entity: &E) -> bool {
        // task #9: the canonical body is factored into a free function so the
        // pipelined `&self` slot-lock read path
        // (`PipelinedWorldServer::user_scope_has_entity_ref`) shares it verbatim
        // — zero semantic drift between the fused and split engines.
        crate::server::user_scope_has_entity_impl(
            &self.shared,
            &self.send.state.entity_scope_map,
            &self.send.state.entity_room_map,
            &self.sim_handle.state.user_store,
            &self.sim_handle.state.resource_registry,
            user_key,
            world_entity,
        )
    }

    //// Components

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        mut component: R,
    ) {
        if !world.has_entity(world_entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component.kind();

        if world.has_component_of_kind(world_entity, &component_kind) {
            // Entity already has this Component type yet, update Component

            let Some(mut component_mut) = world.component_mut::<R>(world_entity) else {
                panic!("Should never happen because we checked for this above");
            };
            component_mut.mirror(&component);
        } else {
            // Entity does not have this Component type yet, initialize Component
            self.insert_component_worldless(world_entity, &mut component);

            // actually insert component into world
            world.insert_component(world_entity, component);
        }
    }

    /// Registers a component insertion in the replication layer without touching the world (adapter use only).
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
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

        // Order matters: register the component GLOBALLY before propagating to
        // per-user scopes. `insert_new_component_into_entity_scopes` triggers
        // per-user `UserDiffHandler::register_component` calls that depend on
        // the global MutChannel existing — without it, `global_handler.receiver`
        // returns None and the per-user notifier is never attached, so mutations
        // on this component for already-in-scope users never reach the per-user
        // diff mask. Symptom: late-inserted components (e.g. NetworkedLastCommand
        // added to an avatar after the entity was already in scope for the
        // observer) silently never replicate their value changes.
        self.shared
            .global_world_manager
            .write()
            .insert_component_record(&global_entity, &component_kind);
        self.shared
            .global_world_manager
            .write()
            .insert_component_diff_handler(&self.shared.component_kinds, &global_entity, component);
        self.insert_new_component_into_entity_scopes(&global_entity, &component_kind, None);

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

    fn insert_new_component_into_entity_scopes(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        excluding_user_opt: Option<&UserKey>,
    ) {
        let excluding_addr_opt: Option<SocketAddr> = {
            if let Some(user_key) = excluding_user_opt {
                self.sim_handle
                    .state
                    .user_store
                    .get(user_key)
                    .map(|user| user.address())
            } else {
                None
            }
        };
        // add component to connections already tracking entity
        for (addr, send_conn) in self.send.state.send_user_connections.iter_mut() {
            if let Some(exclude_addr) = excluding_addr_opt {
                if addr == &exclude_addr {
                    continue;
                }
            }

            // insert component into user's connection
            let has_entity = send_conn
                .base
                .world_manager
                .has_global_entity(global_entity);

            if !has_entity {
                // entity is not in scope for this connection
                continue;
            }
            send_conn
                .base
                .world_manager
                .insert_component(global_entity, component_kind);
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> Option<R> {
        self.remove_component_worldless(world_entity, &ComponentKind::of::<R>());

        // remove from world
        world.remove_component::<R>(world_entity)
    }

    /// Removes a component from the replication layer without touching the world (adapter use only).
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.remove_component_from_all_connections(&global_entity, component_kind);

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

    fn remove_component_from_all_connections(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for (_, send_conn) in self.send.state.send_user_connections.iter_mut() {
            if !send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                // entity is not in scope for this connection
                continue;
            }
            // remove component from user connection
            send_conn
                .base
                .world_manager
                .remove_component(global_entity, component_kind);
        }
    }

    //// Authority

    pub(crate) fn publish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) -> bool {
        if server_origin {
            // send publish message to entity owner
            let entity_owner = self
                .shared
                .global_world_manager
                .read()
                .entity_owner(global_entity);
            let Some(EntityOwner::Client(user_key)) = entity_owner else {
                panic!(
                    "Entity is not owned by a Client. Cannot publish entity. Owner is: {:?}",
                    entity_owner
                );
            };
            // Send PublishEntity action through EntityActionEvent system
            if let Some(user) = self.sim_handle.state.user_store.get(&user_key) {
                if let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                {
                    send_conn
                        .base
                        .world_manager
                        .send_publish(HostType::Server, global_entity);
                }
            }
        }

        let result = self
            .shared
            .global_world_manager
            .write()
            .entity_publish(global_entity);
        if result {
            let entity_map = self.shared.global_entity_map.read();
            world.entity_publish(
                &self.shared.component_kinds,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                world_entity,
            );
            // Re-evaluate scope for every user who shares a room with this entity.
            // The EntityEnteredRoom change was already processed when Private (and
            // returned early); now that the entity is Public we must trigger it again.
            let entity_rooms: Vec<RoomKey> = self
                .send
                .state
                .entity_room_map
                .entity_get_rooms(global_entity)
                .map(|rooms| rooms.iter().copied().collect())
                .unwrap_or_default();
            if !entity_rooms.is_empty() {
                let mut q = self.shared.scope_change_queue.lock();
                for room_key in entity_rooms {
                    q.push_back(ScopeChange::EntityEnteredRoom(*global_entity, room_key));
                }
            }
        }
        result
    }

    pub(crate) fn unpublish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) {
        // Capture the owner's connection address before state change.
        // entity_unpublish() transitions the owner from ClientPublic → Client,
        // so we read it here while it is still ClientPublic.
        let owner_addr: Option<SocketAddr> = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
            .and_then(|o| {
                if let EntityOwner::ClientPublic(k) = o {
                    Some(k)
                } else {
                    None
                }
            })
            .and_then(|k| {
                self.sim_handle
                    .state
                    .user_store
                    .get(&k)
                    .map(|u| u.address())
            });

        if server_origin {
            // Send UnpublishEntity action through EntityActionEvent system
            if let Some(addr) = owner_addr {
                if let Some(send_conn) = self.send.state.send_user_connections.get_mut(&addr) {
                    send_conn
                        .base
                        .world_manager
                        .send_unpublish(HostType::Server, global_entity);
                }
            }
        }

        self.shared
            .global_world_manager
            .write()
            .entity_unpublish(global_entity);
        world.entity_unpublish(world_entity);

        // Deregister each component from the diff handler so re-publishing
        // can register them again without the "cannot Register more than once" panic.
        // 4-F.naia.a: clone the component kinds out so the read guard
        // drops before the .write() acquisition in the loop body
        // (parking_lot RwLock would deadlock on read-held-during-write).
        let kinds_opt: Option<Vec<naia_shared::ComponentKind>> = self
            .shared
            .global_world_manager
            .read()
            .component_kinds(global_entity)
            .map(|k| k.into_iter().collect());
        if let Some(kinds) = kinds_opt {
            for component_kind in kinds {
                self.shared
                    .global_world_manager
                    .write()
                    .remove_component_diff_handler(global_entity, &component_kind);
            }
        }

        // Despawn from non-owner connections only.  Scope map entries and room
        // membership are preserved so a subsequent publish_entity call restores
        // non-owner visibility via room-based scope (entity-publication-11).
        let entity_idx = self.entity_global_idx(global_entity);
        for (addr, send_conn) in self.send.state.send_user_connections.iter_mut() {
            if owner_addr == Some(*addr) {
                continue;
            }
            if send_conn
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                send_conn.base.world_manager.despawn_entity(global_entity);
                send_conn.clear_entity_visible(entity_idx);
            }
        }
    }

    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_origin: Option<UserKey>,
    ) {
        // TODO: check that entity is eligible for delegation?

        {
            // For any users that have this entity in scope,
            // Send an `enable_delegation` message

            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (user_key, user) in self.sim_handle.state.user_store.iter() {
                if let Some(client_key) = &client_origin {
                    if user_key == client_key {
                        // skip sending to origin client, will be handled below
                        continue;
                    }
                }

                let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                else {
                    continue;
                };

                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send EnableDelegationEntity action through EntityActionEvent system
                info!(
                    "Sending EnableDelegation command for entity: {:?} for user: {:?}",
                    global_entity,
                    user.address()
                );
                send_conn.base.world_manager.send_enable_delegation(
                    HostType::Server,
                    client_origin.is_some(),
                    global_entity,
                );
            }
        }

        if let Some(client_key) = client_origin {
            self.enable_delegation_client_owned_entity(
                world,
                global_entity,
                world_entity,
                &client_key,
            );
        } else {
            self.shared
                .global_world_manager
                .write()
                .entity_enable_delegation(global_entity);
            let entity_map = self.shared.global_entity_map.read();
            world.entity_enable_delegation(
                &self.shared.component_kinds,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                world_entity,
            );
        }
    }

    pub(crate) fn enable_delegation_client_owned_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_key: &UserKey,
    ) {
        let Some(entity_owner) = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(global_entity)
        else {
            panic!("entity should have an owner at this point");
        };
        let owner_user_key;
        match entity_owner {
            EntityOwner::Client(user_key) => {
                // The entity was spawned by the client but the Publish packet
                // has not yet arrived (enable-delegation arrived first due to
                // packet reordering). Promote the entity to ClientPublic now so
                // delegation setup can proceed — the Publish packet, when it
                // arrives, will be a no-op since the entity is already public.
                // This is the correct handling for the publish-after-delegation
                // packet-ordering race; it is NOT a shortcut around the protocol.
                owner_user_key = user_key;
                let result = self
                    .shared
                    .global_world_manager
                    .write()
                    .entity_publish(global_entity);
                if !result {
                    warn!(
                        "enable_delegation_client_owned_entity: entity_publish failed for {:?}; \
                         aborting delegation enable (entity may already be public or in an \
                         inconsistent state)",
                        global_entity
                    );
                    return;
                }
                let entity_map = self.shared.global_entity_map.read();
                world.entity_publish(
                    &self.shared.component_kinds,
                    &*entity_map,
                    &*self.shared.global_world_manager.read(),
                    world_entity,
                );
            }
            EntityOwner::ClientPublic(user_key) => {
                owner_user_key = user_key;
            }
            _owner => {
                panic!(
                    "entity should be owned by a public client at this point. Owner is: {:?}",
                    entity_owner
                );
            }
        }
        let user_key = owner_user_key;
        self.shared
            .global_world_manager
            .write()
            .migrate_entity_to_server(global_entity);

        // Initialize the former-owner's scope entry to "in scope" only if it
        // wasn't already set. The check at the end of this method consults
        // `entity_scope_map` directly to decide whether to grant initial
        // authority to the former owner — overwriting an explicit exclude
        // would silently grant authority to a user who had been put
        // out-of-scope by the application (contract
        // [entity-delegation-09]: "migration yields no holder if owner is
        // out of scope at migration time").
        if self
            .send
            .state
            .entity_scope_map
            .get(&user_key, global_entity)
            .is_none()
        {
            self.send
                .state
                .entity_scope_map
                .insert(user_key, *global_entity, true);
        }

        // Migrate Entity from Remote -> Host connection
        let Some(user) = self.sim_handle.state.user_store.get(&user_key) else {
            panic!("user should exist");
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            panic!("connection does not exist")
        };

        // Step 0: Capture old RemoteEntity BEFORE migration (will be needed for MigrateResponse)
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

        // Step 1: Migrate entity from RemoteEntity to HostEntity
        // This creates the HostEntity in HostEngine so it can receive commands
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

        // Step 2: Force the server's HostEntityChannel into Delegated state locally
        // This allows MigrateResponse to be sent (requires Delegated state)
        // We do NOT send EnableDelegation back to the client - they already sent it!
        send_conn
            .base
            .world_manager
            .host_local_enable_delegation(&new_host_entity);

        // Step 3: Send MigrateResponse to client
        // This will be the FIRST message in the new HostEntityChannel sequence (subcommand_id=0)
        send_conn.base.world_manager.host_send_migrate_response(
            global_entity,
            &old_remote_entity,
            &new_host_entity,
        );

        self.shared
            .global_world_manager
            .write()
            .entity_enable_delegation(global_entity);
        let entity_map = self.shared.global_entity_map.read();
        world.entity_enable_delegation(
            &self.shared.component_kinds,
            &*entity_map,
            &*self.shared.global_world_manager.read(),
            world_entity,
        );

        // Per contracts [entity-delegation-06]/[07]/[08]/[09]: the
        // previous owner gets initial Granted authority *iff* it's
        // still in-scope for the entity at migration time. If the
        // owner is out-of-scope, no holder is assigned and every
        // in-scope client observes Available (the default emitted by
        // EnableDelegation). We use `entity_scope_map` directly
        // because `user_scope_has_entity` takes a world_entity (E),
        // not a global_entity, and we only have the global here.
        let owner_in_scope = self
            .send
            .state
            .entity_scope_map
            .get(client_key, global_entity)
            .copied()
            .unwrap_or(false);

        if owner_in_scope {
            let requester = AuthOwner::from_user_key(Some(client_key));
            let result = self
                .shared
                .global_world_manager
                .write()
                .client_request_authority(global_entity, &requester);
            if result.is_err() {
                panic!(
                    "failed to grant authority of client-owned delegated entity to creating user"
                );
            }

            // Fan out SetAuthority to every in-scope user so the holder
            // observes Granted and everyone else observes Denied.
            // Without this, the per-client auth status stays at the
            // EnableDelegation default (Available) and contracts
            // [entity-delegation-06]/[entity-delegation-07] (migration
            // assigns initial authority to the previous owner) silently
            // fail. Snapshot first so we can re-borrow user_connections
            // mutably inside the loop.
            let user_snapshot: Vec<(UserKey, std::net::SocketAddr)> = self
                .sim_handle
                .state
                .user_store
                .iter()
                .map(|(k, u)| (*k, u.address()))
                .collect();
            for (user_key, addr) in user_snapshot {
                let Some(send_conn) = self.send.state.send_user_connections.get_mut(&addr) else {
                    continue;
                };
                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    continue;
                }
                let new_status = if user_key == *client_key {
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
        // else: owner is out-of-scope — leave AuthOwner::None and don't
        // emit any SetAuthority. Every in-scope client already sees
        // Available from the EnableDelegation default.
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
    ) {
        // TODO: check that entity is eligible for delegation?

        // for any users that have this entity in scope, send an `disable_delegation` message
        {
            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (_user_key, user) in self.sim_handle.state.user_store.iter() {
                let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                else {
                    continue;
                };

                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send DisableDelegationEntity action through EntityActionEvent system
                send_conn
                    .base
                    .world_manager
                    .send_disable_delegation(global_entity);
            }
        }

        self.shared
            .global_world_manager
            .write()
            .entity_disable_delegation(global_entity);
        world.entity_disable_delegation(world_entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        self.sim_handle.user_address(user_key)
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub(crate) fn user_room_keys(&'_ self, user_key: &UserKey) -> Option<Iter<'_, RoomKey>> {
        self.sim_handle.state.user_store.room_keys_iter(user_key)
    }

    /// Get an count of how many Rooms the given User is inside
    pub(crate) fn user_rooms_count(&self, user_key: &UserKey) -> Option<usize> {
        self.sim_handle.state.user_store.rooms_count(user_key)
    }

    pub(crate) fn user_disconnect<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        reason: DisconnectReason,
        world: &mut W,
    ) {
        if self.shared.client_authoritative_entities {
            self.despawn_all_remote_entities(user_key, world);
            // 4-F.naia.a: clone the owned-entity set out from under the
            // gwm read guard so we can call `entity_release_authority`
            // (which takes `&mut self`) below without holding the lock.
            let copied_entities_opt: Option<Vec<GlobalEntity>> = self
                .shared
                .global_world_manager
                .read()
                .user_all_owned_entities(user_key)
                .map(|s| s.iter().copied().collect());
            if let Some(copied_entities) = copied_entities_opt {
                for global_entity in copied_entities {
                    // Only release authority if entity still exists (may have been despawned already).
                    // Resolve world_entity in a scoped read so the guard drops before
                    // the mutating `entity_release_authority` call.
                    let world_entity_opt = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .ok();
                    if let Some(world_entity) = world_entity_opt {
                        let _ = self.entity_release_authority(Some(user_key), &world_entity);
                    }
                }
            }
        }
        let user = self.user_delete(user_key);
        self.recv
            .state
            .incoming_world_events
            .push_disconnection(user_key, user.address(), reason);
    }

    pub(crate) fn user_queue_disconnect(&mut self, user_key: &UserKey, reason: DisconnectReason) {
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            // User already disconnected, this is fine (disconnect packets may arrive multiple times)
            return;
        };
        let Some(recv_conn) = self
            .recv
            .state
            .recv_user_connections
            .get_mut(&user.address())
        else {
            // Connection already gone, user is being/has been disconnected
            return;
        };

        // If already marked for disconnect, don't queue again (idempotent)
        if recv_conn.manual_disconnect {
            return;
        }

        recv_conn.manual_disconnect = true;
        // Add to outstanding_disconnects immediately so it gets processed in the next process_all_packets call
        self.recv
            .state
            .outstanding_disconnects
            .push((*user_key, reason));
    }

    pub(crate) fn user_delete(&mut self, user_key: &UserKey) -> WorldUser {
        let Some(user) = self.sim_handle.state.user_store.remove(user_key) else {
            panic!("Attempting to delete non-existent user!");
        };

        let user_addr = user.address();

        info!("deleting authenticated user for {}", user.address());
        // 4-E.2d: drop recv-side directly (coordinator owns the recv map
        // in pipeline mode too).
        // 4-E.2e: send-side removal routes through the SendStateUpdate
        // queue so the coordinator never reaches into the send-side
        // map; drained inline below for same-cycle visibility.
        self.recv.state.recv_user_connections.remove(&user_addr);
        self.shared
            .pending_send_state_updates
            .lock()
            .push(crate::server::SendStateUpdate::ConnectionRemoved(user_addr));
        self.commit_pending_send_state_updates();

        // Drop this user's entire per-user priority layer so entries never
        // leak across user sessions.
        self.send.state.user_priorities.remove(user_key);

        self.send.state.entity_scope_map.remove_user(user_key);

        // Clean up all user data
        for room_key in user.room_keys() {
            self.sim_handle
                .state
                .room_store
                .get_mut(room_key)
                .unwrap()
                .unsubscribe_user(user_key);
            // Mirror the room→user removal into the scope-checks cache —
            // this path bypasses `room_remove_user`.
            self.send
                .state
                .scope_checks_cache
                .on_user_removed_from_room(*room_key, *user_key);
        }

        // remove from bandwidth monitor
        if self.send.state.send_io.bandwidth_monitor_enabled() {
            self.recv.state.recv_io.deregister_client(&user.address());
            self.send.state.send_io.deregister_client(&user.address());
        }

        self.sim_handle
            .state
            .global_request_manager
            .purge_user(user_key);
        self.sim_handle
            .state
            .global_response_manager
            .purge_user(user_key);

        user
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn despawn_all_remote_entities<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            panic!("Attempting to despawn entities for a nonexistent user");
        };
        let Some(send_conn) = self
            .send
            .state
            .send_user_connections
            .get_mut(&user.address())
        else {
            panic!("Attempting to despawn entities on a nonexistent connection");
        };

        let remote_global_entities = send_conn.base.world_manager.remote_entities();
        let entity_events = {
            let entity_map = self.shared.global_entity_map.read();
            SharedGlobalWorldManager::despawn_all_entities(
                world,
                &*entity_map,
                &*self.shared.global_world_manager.read(),
                remote_global_entities,
            )
        };
        self.process_entity_events(world, user_key, entity_events);
    }

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        let (existed, room_change_opt) = {
            let entity_map = self.shared.global_entity_map.read();
            self.sim_handle.state.room_store.destroy(
                room_key,
                &mut self.sim_handle.state.user_store,
                &*entity_map,
            )
        };
        if let Some(room_change) = room_change_opt {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(ScopeChange::RoomChange(room_change));
        }
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
        existed
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        self.sim_handle
            .state
            .room_store
            .has_user(room_key, user_key)
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        let (legacy_change, room_change) = {
            let entity_map = self.shared.global_entity_map.read();
            self.sim_handle.state.room_store.add_user(
                room_key,
                user_key,
                &mut self.sim_handle.state.user_store,
                &*entity_map,
            )
        };
        {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        let (legacy_change, room_change) = self.sim_handle.state.room_store.remove_user::<E>(
            room_key,
            user_key,
            &mut self.sim_handle.state.user_store,
        );
        {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: &RoomKey) -> usize {
        self.sim_handle.state.room_store.users_count(room_key)
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the Room
    pub(crate) fn room_user_keys(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        self.sim_handle.state.room_store.user_keys_iter(room_key)
    }

    pub(crate) fn room_entities(&self, room_key: &RoomKey) -> impl Iterator<Item = &GlobalEntity> {
        self.sim_handle.state.room_store.entities_iter(room_key)
    }

    /// Sends a message to all connected users in a given Room using a given channel
    pub(crate) fn room_broadcast_message(
        &mut self,
        channel_kind: &ChannelKind,
        room_key: &RoomKey,
        message_box: Box<dyn Message>,
    ) {
        // Wrap once in Arc so per-user clones are refcount increments, not heap allocs.
        let container = MessageContainer::new(message_box);
        let user_keys: Vec<UserKey> = self
            .sim_handle
            .state
            .room_store
            .user_keys_iter(room_key)
            .cloned()
            .collect();
        for user_key in &user_keys {
            let _ = self.send_message_inner(user_key, channel_kind, container.clone());
        }
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity: &GlobalEntity) -> bool {
        self.sim_handle
            .state
            .room_store
            .has_entity(room_key, entity)
    }

    /// Add an Entity to a Room associated with the given RoomKey.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.sim_handle
                .state
                .room_store
                .add_entity(room_key, world_entity, &*entity_map)
        };
        if let Some((legacy_change, room_change)) = pair_opt {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Remove an Entity from a Room, associated with the given RoomKey
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.sim_handle
                .state
                .room_store
                .remove_entity(room_key, world_entity, &*entity_map)
        };
        if let Some((legacy_change, room_change)) = pair_opt {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
        self.send
            .state
            .apply_pending_room_changes(&self.shared.scope_change_queue);
    }

    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        self.sim_handle.state.room_store.entities_count(room_key)
    }

    // Private methods

    /// Recv-stage handler for a Data packet. Runs on the recv thread.
    ///
    /// 4-E.2e: recv-side does the bare minimum — process the standard
    /// header (publishes ACK snapshot on the shared atomic), read the
    /// client tick out of the wire stream, and buffer the remaining
    /// reader into `RecvState::pending_data_packets`. The coordinator
    /// thread later drains via `decode_pending_data_packets` (serial:
    /// inline at recv tail; pipeline: 4-E.2f wires
    /// `SendHandle::process_recv_packets`).
    // 4-F.naia.c.2b: `read_data_packet` moved to `RecvState::read_data_packet`
    // (recv-only) so `RecvState::receive` can drive it without crossing
    // halves.

    // 4-F.naia.c.2b: the former `decode_pending_data_packets` method has
    // moved to `SendState::process_recv_packets`, which is the canonical
    // home for cross-half post-recv processing. The two halves are still
    // passed in together; the difference is that `&mut RecvConnection`
    // arrives as an explicit parameter rather than via `self.recv.state`, which
    // lets `SendHandle` run the same body from the pipeline coordinator.

    fn process_disconnects<W: WorldMutType<E>>(&mut self, world: &mut W) {
        // MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — drain
        // disconnect requests queued by `CoordHandle::disconnect_user`. Collect
        // while holding the lock, then call `user_queue_disconnect` without
        // the lock (it may re-lock other shared state). Runs immediately before
        // `outstanding_disconnects` is consumed so both are processed in the
        // same recv tick.
        let pending: Vec<(crate::user::UserKey, DisconnectReason)> =
            std::mem::take(&mut *self.shared.pending_disconnect_requests.lock());
        for (user_key, reason) in pending {
            self.user_queue_disconnect(&user_key, reason);
        }

        let user_disconnects = std::mem::take(&mut self.recv.state.outstanding_disconnects);
        for (user_key, reason) in user_disconnects {
            self.user_disconnect(&user_key, reason, world);
        }
    }

    fn process_packets<W: WorldMutType<E>>(
        &mut self,
        address: &SocketAddr,
        world: &mut W,
        now: &Instant,
    ) {
        // Packets requiring established connection — process_packets lives
        // on SendConnection (the half that owns base.message_manager +
        // world_manager).
        let (user_key, entity_events) = {
            let mut entity_map = self.shared.global_entity_map.write();
            let Some(send_conn) = self.send.state.send_user_connections.get_mut(address) else {
                return;
            };
            (
                send_conn.user_key,
                send_conn.process_packets(
                    &self.shared.message_kinds,
                    &self.shared.component_kinds,
                    self.shared.client_authoritative_entities,
                    now,
                    &mut *entity_map,
                    &mut *self.shared.global_world_manager.write(),
                    &mut self.sim_handle.state.global_request_manager,
                    &mut self.sim_handle.state.global_response_manager,
                    world,
                    &mut self.recv.state.incoming_world_events,
                ),
            )
        };
        self.process_entity_events(world, &user_key, entity_events);
    }

    fn process_entity_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        user_key: &UserKey,
        response_events: Vec<EntityEvent>,
    ) {
        let mut deferred_events = Vec::new();
        for response_event in response_events {
            match response_event {
                EntityEvent::Spawn(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.recv
                        .state
                        .incoming_world_events
                        .push_spawn(user_key, &world_entity);
                    let idx = self
                        .shared
                        .global_world_manager
                        .write()
                        .insert_entity_record(&global_entity, EntityOwner::Client(*user_key));
                    if idx.is_valid() {
                        self.shared.idx_to_world.write()[idx.as_usize()] = Some(world_entity);
                    }
                    let user = self.sim_handle.state.user_store.get(user_key).unwrap();
                    let send_conn = self
                        .send
                        .state
                        .send_user_connections
                        .get_mut(&user.address())
                        .unwrap();
                    send_conn
                        .base
                        .world_manager
                        .remote_spawn_entity(&global_entity); // TODO: migrate to localworldmanager
                    #[cfg(feature = "e2e_debug")]
                    {
                        SERVER_SPAWN_APPLIED.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    // Fire synthetic remove events for each component before despawn.
                    // Symmetric to the client-side process_remove ordering: RemoveComponentEvent
                    // must arrive before DespawnEntityEvent so observers can act on the component.
                    // The server does not hold component data for client entities, so we emit
                    // kind-only remove events via push_remove_synthetic.
                    if let Some(component_kinds) = self
                        .shared
                        .global_world_manager
                        .read()
                        .component_kinds(&global_entity)
                    {
                        for component_kind in component_kinds {
                            self.recv.state.incoming_world_events.push_remove_synthetic(
                                user_key,
                                &world_entity,
                                &component_kind,
                            );
                        }
                    }
                    self.recv
                        .state
                        .incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    deferred_events.push(EntityEvent::Despawn(global_entity));
                }
                EntityEvent::InsertComponent(global_entity, component_kind) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.recv.state.incoming_world_events.push_insert(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                    self.shared
                        .global_world_manager
                        .write()
                        .insert_component_record(
                            // &self.shared.component_kinds,
                            &global_entity,
                            &component_kind,
                        );
                    let is_public_and_client_owned = self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_is_public_and_client_owned(&global_entity);
                    let is_delegated = self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_is_delegated(&global_entity);

                    if is_public_and_client_owned || is_delegated {
                        let entity_map = self.shared.global_entity_map.read();
                        world.component_publish(
                            &self.shared.component_kinds,
                            &*entity_map,
                            &*self.shared.global_world_manager.read(),
                            &world_entity,
                            &component_kind,
                        );

                        if is_delegated {
                            world.component_enable_delegation(
                                &self.shared.component_kinds,
                                &*entity_map,
                                &*self.shared.global_world_manager.read(),
                                &world_entity,
                                &component_kind,
                            );
                        }
                        drop(entity_map);

                        self.insert_new_component_into_entity_scopes(
                            &global_entity,
                            &component_kind,
                            Some(user_key),
                        );
                    }
                }
                EntityEvent::RemoveComponent(global_entity, component) => {
                    let component_kind = component.kind();
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.recv.state.incoming_world_events.push_remove(
                        user_key,
                        &world_entity,
                        component,
                    );
                    if self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_is_public_and_client_owned(&global_entity)
                        || self
                            .shared
                            .global_world_manager
                            .read()
                            .entity_is_delegated(&global_entity)
                    {
                        self.remove_component_worldless(&world_entity, &component_kind);
                    } else {
                        self.shared
                            .global_world_manager
                            .write()
                            .remove_component_record(&global_entity, &component_kind);
                    }
                }
                EntityEvent::UpdateComponent(_tick, global_entity, component_kind) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.recv.state.incoming_world_events.push_update(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                }
                _ => {
                    deferred_events.push(response_event);
                }
            }
        }

        let mut extra_deferred_events = Vec::new();
        // The reason for deferring these events is that they depend on the operations to the world above
        for response_event in deferred_events {
            match response_event {
                EntityEvent::Publish(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    // Entity may have been despawned in the same message batch before
                    // this deferred event fires; skip world operations if it's gone.
                    if !world.has_entity(&world_entity) {
                        continue;
                    }
                    self.publish_entity(world, &global_entity, &world_entity, false);
                    self.recv
                        .state
                        .incoming_world_events
                        .push_publish(user_key, &world_entity);

                    // NOTE: Client-owned entities do NOT get auto-granted authority.
                    // Authority/SetAuthority only applies to delegated (server-owned) entities.
                }
                EntityEvent::Unpublish(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if !world.has_entity(&world_entity) {
                        continue;
                    }
                    self.unpublish_entity(world, &global_entity, &world_entity, false);
                    self.recv
                        .state
                        .incoming_world_events
                        .push_unpublish(user_key, &world_entity);
                }
                EntityEvent::EnableDelegation(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    // Entity may have been despawned in the same message batch
                    // (Despawn arrives alongside EnableDelegation but the Bevy
                    // entity was already removed by process_ready_messages while
                    // the Despawn event is still deferred).  Skip delegation so
                    // we don't call component_kinds on a stale entity.
                    if !world.has_entity(&world_entity) {
                        continue;
                    }
                    self.entity_enable_delegation(
                        world,
                        &global_entity,
                        &world_entity,
                        Some(*user_key),
                    );
                    self.recv
                        .state
                        .incoming_world_events
                        .push_delegate(user_key, &world_entity);
                }
                EntityEvent::EnableDelegationResponse(global_entity) => {
                    self.entity_enable_delegation_response(user_key, &global_entity);
                }
                EntityEvent::DisableDelegation(_) => {
                    panic!("Clients should not be able to disable entity delegation.");
                }
                EntityEvent::RequestAuthority(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if self
                        .entity_handle_client_request_authority(user_key, &world_entity)
                        .is_err()
                    {
                        // Tell the requester it lost. Without this the client
                        // that asked stays in `Requested` forever: the grant
                        // path notifies every user, but the rejection path used
                        // to be server-local, so a contended request produced no
                        // EntityAuthDeniedEvent on the client that was refused.
                        self.notify_user_auth_denied(user_key, &global_entity);
                        self.recv
                            .state
                            .incoming_world_events
                            .push_auth_denied(user_key, &world_entity);
                    }
                }
                EntityEvent::ReleaseAuthority(global_entity) => {
                    // info!("received release auth entity message!");
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if self
                        .entity_release_authority(Some(user_key), &world_entity)
                        .is_ok()
                    {
                        self.recv
                            .state
                            .incoming_world_events
                            .push_auth_reset(&world_entity);
                    }
                }
                EntityEvent::SetAuthority(_, _) => {
                    panic!("Clients should not be able to update entity authority.");
                }
                EntityEvent::MigrateResponse(_, _) => {
                    panic!("Clients should not be able to send this message");
                }
                _ => {
                    extra_deferred_events.push(response_event);
                }
            }
        }

        for response_event in extra_deferred_events {
            match response_event {
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .shared
                        .global_entity_map
                        .read()
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.recv
                        .state
                        .incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    let owner = self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_owner(&global_entity);
                    let is_delegated = self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_is_delegated(&global_entity);
                    let is_pub_client_owned = self
                        .shared
                        .global_world_manager
                        .read()
                        .entity_is_public_and_client_owned(&global_entity);
                    if is_pub_client_owned && !is_delegated {
                        // Non-delegated public client entity: tracked in the host entity map
                        // (not the remote entity map), so remote_despawn_entity would panic.
                        // Just do the full teardown directly.
                        self.despawn_entity_worldless(&world_entity);
                    } else if is_delegated
                        && matches!(
                            owner,
                            Some(
                                EntityOwner::Client(_)
                                    | EntityOwner::ClientPublic(_)
                                    | EntityOwner::ClientWaiting(_)
                            )
                        )
                    {
                        // Client-created delegated entity: remove from this connection's remote
                        // world manager, then tear down the server-side entity record entirely.
                        let user = self.sim_handle.state.user_store.get(user_key).unwrap();
                        let send_conn = self
                            .send
                            .state
                            .send_user_connections
                            .get_mut(&user.address())
                            .unwrap();
                        send_conn
                            .base
                            .world_manager
                            .remote_despawn_entity(&global_entity);

                        self.despawn_entity_worldless(&world_entity);
                    } else if is_delegated {
                        // Server-created delegated entity despawned by the authority-holding client.
                        // The entity lives in the server's host world manager, not in any remote
                        // world manager, so skip remote_despawn_entity and go straight to full teardown.
                        self.despawn_entity_worldless(&world_entity);
                    } else {
                        self.shared
                            .global_world_manager
                            .write()
                            .remove_entity_record(&global_entity);
                        self.shared
                            .global_entity_map
                            .write()
                            .despawn_by_global(&global_entity);
                    }
                }
                _ => {
                    panic!("shouldn't happen");
                }
            }
        }
    }

    // Entity Scopes

    fn update_entity_scopes<W: WorldRefType<E>>(&mut self, world: &W) {
        // Loop 1 (both paths): drain per-room entity-removal queues.
        // This handles entities removed from a room via room_remove_entity.
        // Clone the diff_handler Arc once for the entire loop.
        let diff_handler_arc = self.shared.global_world_manager.read().diff_handler();
        for (_, room) in self.sim_handle.state.room_store.iter_mut() {
            while let Some((removed_user, removed_global_entity)) = room.pop_entity_removal_queue()
            {
                let Some(user) = self.sim_handle.state.user_store.get(&removed_user) else {
                    continue;
                };
                let Some(send_conn) = self
                    .send
                    .state
                    .send_user_connections
                    .get_mut(&user.address())
                else {
                    continue;
                };

                // evaluate whether the Entity really needs to be despawned!
                // what if the Entity shares another Room with this User? It shouldn't be despawned!
                if let Some(entity_rooms) = self
                    .send
                    .state
                    .entity_room_map
                    .entity_get_rooms(&removed_global_entity)
                {
                    let user_rooms = user.room_keys();
                    let has_room_in_common = entity_rooms.intersection(user_rooms).next().is_some();
                    if has_room_in_common {
                        continue;
                    }
                }

                // check if host has entity, because it may have been removed from room before despawning, and we don't want to double despawn
                if !send_conn
                    .base
                    .world_manager
                    .has_global_entity(&removed_global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                let entity_idx = {
                    let guard = diff_handler_arc
                        .read()
                        .expect("GlobalDiffHandler lock poisoned");
                    guard
                        .entity_to_global_idx(&removed_global_entity)
                        .unwrap_or(GlobalEntityIndex::INVALID)
                };

                // remove entity from user connection
                send_conn
                    .base
                    .world_manager
                    .despawn_entity(&removed_global_entity);
                send_conn.clear_entity_visible(entity_idx);
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Loop 2: process queued scope changes.
        self.drain_scope_change_queue(world);
    }

    fn drain_scope_change_queue<W: WorldRefType<E>>(&mut self, world: &W) {
        // Snapshot the queue so we can re-borrow self mutably for apply_scope_for_user.
        let changes: Vec<ScopeChange<E>> =
            self.shared.scope_change_queue.lock().drain(..).collect();
        #[cfg(feature = "f3_diag")]
        if !changes.is_empty() {
            let summary: Vec<String> = changes
                .iter()
                .map(|c| match c {
                    ScopeChange::UserEnteredRoom(u, r) => {
                        format!("UserEnteredRoom({:?},{:?})", u, r)
                    }
                    ScopeChange::UserLeftRoom(u, r) => format!("UserLeftRoom({:?},{:?})", u, r),
                    ScopeChange::EntityEnteredRoom(e, r) => {
                        format!("EntityEnteredRoom({:?},{:?})", e, r)
                    }
                    ScopeChange::ScopeToggled(u, e, b) => {
                        format!("ScopeToggled({:?},{:?},{})", u, e, b)
                    }
                    ScopeChange::RoomChange(_) => "RoomChange(_)".to_string(),
                    ScopeChange::ConfigureReplication(_) => "ConfigureReplication(_)".to_string(),
                })
                .collect();
            eprintln!(
                "[F3-DIAG naia/InternalWorldServer] drain_scope_change_queue draining {} variants: {:?}",
                changes.len(),
                summary
            );
        }
        for change in changes {
            match change {
                ScopeChange::UserEnteredRoom(user_key, room_key) => {
                    let entity_list: Vec<GlobalEntity> = self
                        .sim_handle
                        .state
                        .room_store
                        .get(&room_key)
                        .map(|r| r.entities().copied().collect())
                        .unwrap_or_default();
                    for global_entity in &entity_list {
                        self.apply_scope_for_user(world, &user_key, global_entity);
                    }
                }
                ScopeChange::UserLeftRoom(user_key, room_key) => {
                    let entity_list: Vec<GlobalEntity> = self
                        .sim_handle
                        .state
                        .room_store
                        .get(&room_key)
                        .map(|r| r.entities().copied().collect())
                        .unwrap_or_default();
                    let Some(user) = self.sim_handle.state.user_store.get(&user_key) else {
                        continue;
                    };
                    let user_rooms = user.room_keys().clone();
                    // Clone the diff_handler Arc so we can read it inside the entity loop
                    // without conflicting with the mutable connection borrow below.
                    let diff_handler_arc = self.shared.global_world_manager.read().diff_handler();
                    let Some(send_conn) = self
                        .send
                        .state
                        .send_user_connections
                        .get_mut(&user.address().clone())
                    else {
                        continue;
                    };
                    for global_entity in &entity_list {
                        // Only despawn if the user has no other room in common with the entity.
                        if let Some(entity_rooms) = self
                            .send
                            .state
                            .entity_room_map
                            .entity_get_rooms(global_entity)
                        {
                            if entity_rooms.iter().any(|rk| user_rooms.contains(rk)) {
                                continue;
                            }
                        }
                        if !send_conn
                            .base
                            .world_manager
                            .has_global_entity(global_entity)
                        {
                            continue;
                        }
                        let entity_idx = {
                            let guard = diff_handler_arc
                                .read()
                                .expect("GlobalDiffHandler lock poisoned");
                            guard
                                .entity_to_global_idx(global_entity)
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
                        #[cfg(feature = "e2e_debug")]
                        {
                            SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                ScopeChange::EntityEnteredRoom(global_entity, room_key) => {
                    let user_keys: Vec<UserKey> = self
                        .sim_handle
                        .state
                        .room_store
                        .get(&room_key)
                        .map(|r| r.user_keys().copied().collect())
                        .unwrap_or_default();
                    for user_key in &user_keys {
                        self.apply_scope_for_user(world, user_key, &global_entity);
                    }
                }
                ScopeChange::ScopeToggled(user_key, global_entity, _is_included) => {
                    self.apply_scope_for_user(world, &user_key, &global_entity);
                }
                ScopeChange::RoomChange(_) => {
                    unreachable!(
                        "apply_pending_room_changes must run before drain_scope_change_queue"
                    );
                }
                ScopeChange::ConfigureReplication(_) => {
                    // D.2.2: only the Coord-only `CoordHandle::configure_
                    // entity_replication` pushes this variant; the legacy
                    // fused `InternalWorldServer::configure_entity_replication`
                    // path is fully synchronous and never enqueues it.
                    unreachable!(
                        "ConfigureReplication is never pushed on the fused InternalWorldServer path"
                    );
                }
            }
        }
    }

    /// Evaluate scope for one (user, entity) pair and apply any spawn/despawn/pause/resume.
    fn apply_scope_for_user<W: WorldRefType<E>>(
        &mut self,
        world: &W,
        user_key: &UserKey,
        global_entity: &GlobalEntity,
    ) {
        // Resolve GlobalEntityIndex before any mutable borrows on self.
        let entity_idx = self.entity_global_idx(global_entity);

        let Some(user) = self.sim_handle.state.user_store.get(user_key) else {
            #[cfg(feature = "f3_diag")]
            eprintln!(
                "[F3-DIAG naia/apply_scope_for_user] EARLY user={:?} ge={:?} reason=user_not_found",
                user_key, global_entity
            );
            return;
        };
        let user_addr = user.address();
        let Some(send_conn) = self.send.state.send_user_connections.get_mut(&user_addr) else {
            #[cfg(feature = "f3_diag")]
            eprintln!("[F3-DIAG naia/apply_scope_for_user] EARLY user={:?} addr={:?} ge={:?} reason=no_send_conn", user_key, user_addr, global_entity);
            return;
        };
        let Some(world_entity) = self
            .shared
            .global_entity_map
            .read()
            .global_entity_to_entity(global_entity)
            .ok()
        else {
            #[cfg(feature = "f3_diag")]
            eprintln!("[F3-DIAG naia/apply_scope_for_user] EARLY user={:?} ge={:?} reason=no_world_entity_mapping", user_key, global_entity);
            return;
        };
        if !world.has_entity(&world_entity) {
            #[cfg(feature = "f3_diag")]
            eprintln!("[F3-DIAG naia/apply_scope_for_user] REQUEUE user={:?} ge={:?} reason=world.has_entity=false", user_key, global_entity);
            // Entity not yet spawned in Bevy (deferred commands still pending).
            // Re-queue so we retry next frame instead of permanently losing the scope change.
            self.shared
                .scope_change_queue
                .lock()
                .push_back(ScopeChange::ScopeToggled(*user_key, *global_entity, true));
            return;
        }
        if self
            .shared
            .global_world_manager
            .read()
            .entity_is_public_and_owned_by_user(user_key, global_entity)
        {
            // entity is owned by client but public — don't replicate via this path
            return;
        }
        // Per [entity-publication]: Private (Client/ClientWaiting) entities must
        // never be replicated via this path.
        if matches!(
            self.shared
                .global_world_manager
                .read()
                .entity_owner(global_entity),
            Some(EntityOwner::Client(_)) | Some(EntityOwner::ClientWaiting(_))
        ) {
            return;
        }

        // Visibility-based scope state:
        //   currently_visible = entity is actively in scope (tracked AND not paused)
        //   is_tracked         = entity is in entity_map (active OR paused)
        //   currently_paused   = tracked but not active
        let currently_visible = send_conn.visibility.is_set(entity_idx);
        let is_tracked = send_conn
            .base
            .world_manager
            .has_global_entity(global_entity);
        let currently_paused = is_tracked && !currently_visible;

        // Decide scope membership. Per contract [entity-scopes-06] /
        // [entity-scopes-12]: an explicit user-scope override wins
        // over the room-default rule. Three cases:
        //   - explicit override = Some(true)  → in scope (even if no
        //     room overlap; "include overrides room absence")
        //   - explicit override = Some(false) → out of scope (even with
        //     room overlap; "exclude hides despite shared room")
        //   - explicit override = None        → use the room default
        //     (in scope iff user and entity share a room)
        // Replicated Resources (D14 / §4.3 of RESOURCES_PLAN) bypass
        // the room rule entirely and are unconditionally in-scope for
        // every connected user, but the explicit-exclude override still
        // applies defensively.
        let in_common_room = if let Some(entity_rooms) = self
            .send
            .state
            .entity_room_map
            .entity_get_rooms(global_entity)
        {
            entity_rooms.intersection(user.room_keys()).next().is_some()
        } else {
            false
        };
        let explicit = self
            .send
            .state
            .entity_scope_map
            .get(user_key, global_entity)
            .copied();
        let is_resource = self
            .sim_handle
            .state
            .resource_registry
            .is_resource_entity(global_entity);
        // [entity-scopes-09]: explicit include() MUST NOT bypass the room gate for
        // server-owned entities that have no rooms at all. If the entity has rooms
        // (even rooms the user isn't in), include() is a valid cross-room override
        // per [entity-scopes-06]. Resources and client-owned entities are exempt.
        let entity_is_roomless = self
            .send
            .state
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
        #[cfg(feature = "f3_diag")]
        eprintln!("[F3-DIAG naia/apply_scope_for_user] DECIDE user={:?} ge={:?} should_be_in_scope={} currently_visible={} currently_paused={} in_common_room={} explicit={:?} is_resource={}", user_key, global_entity, should_be_in_scope, currently_visible, currently_paused, in_common_room, explicit, is_resource);
        if should_be_in_scope {
            if currently_visible {
                // Entity already active — no change needed.
                return;
            }
            if currently_paused {
                // Re-entering scope on a paused (ScopeExit::Persist) entity.
                send_conn.base.world_manager.resume_entity(global_entity);
                send_conn.set_entity_visible(entity_idx);
                return;
            }
            // Entity not yet tracked for this connection — enter scope.
            let component_kinds = self
                .shared
                .global_world_manager
                .read()
                .component_kinds(global_entity)
                .unwrap();
            #[cfg(feature = "f3_diag")]
            eprintln!("[F3-DIAG naia/apply_scope_for_user] HOST_INIT user={:?} ge={:?} component_kinds.len={}", user_key, global_entity, component_kinds.len());
            send_conn.base.world_manager.host_init_entity(
                global_entity,
                component_kinds,
                &self.shared.component_kinds,
                self.shared
                    .global_world_manager
                    .read()
                    .entity_is_static(global_entity),
            );
            send_conn.set_entity_visible(entity_idx);
            #[cfg(feature = "e2e_debug")]
            {
                SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            }

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
            // Re-entering scope on a delegated entity that already has a
            // holder must surface the current holder's state to the
            // freshly-included user — otherwise the EnableDelegation
            // default of Available silently overrides the real Denied
            // status. Per contract [entity-delegation-15] / scope-re-entry:
            // "re-entering scope yields current authority status".
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
            // Entity leaving active scope — check ScopeExit policy.
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
            #[cfg(feature = "e2e_debug")]
            {
                SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            }
            // Priority layer eviction: this user's per-user priority entry for
            // this entity is scoped to in-scope lifetime. Drop it regardless
            // of scope-exit policy — a Persist pause still means no outbound
            // traffic for this (user, entity) pair until re-scoped.
            if let Some(layer) = self.send.state.user_priorities.get_mut(user_key) {
                layer.on_scope_exit(&world_entity);
            }
        }
    }

    /// Look up the dense `GlobalEntityIndex` for `global_entity` from the diff handler.
    /// Returns `GlobalEntityIndex::INVALID` if the entity is not yet registered.
    fn entity_global_idx(&self, global_entity: &GlobalEntity) -> GlobalEntityIndex {
        let handler = self.shared.global_world_manager.read().diff_handler();
        let guard = handler.read().expect("GlobalDiffHandler lock poisoned");
        guard
            .entity_to_global_idx(global_entity)
            .unwrap_or(GlobalEntityIndex::INVALID)
    }

    // 4-F.naia.c.2b: `handle_disconnects` moved to
    // `RecvState::handle_disconnects` (recv-only).
}

pub(crate) fn user_connection_ready_impl(
    user_store: &UserStore,
    send_user_connections: &std::collections::HashMap<
        std::net::SocketAddr,
        crate::connection::SendConnection,
    >,
    user_key: &UserKey,
) -> bool {
    let Some(user) = user_store.get(user_key) else {
        return false;
    };
    send_user_connections.contains_key(&user.address())
}

impl<E: Hash + Copy + Eq + Sync + Send> EntityAndGlobalEntityConverter<E>
    for InternalWorldServer<E>
{
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        // 4-E.2c: read guard lives only for the call. Returns owned `E`
        // (Copy) — no borrow escapes the guard.
        self.shared
            .global_entity_map
            .read()
            .global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        // Same shape — returns owned `GlobalEntity`.
        self.shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
    }
}

cfg_if! {
    if #[cfg(feature = "test_utils")] {
        impl<E: Copy + Eq + Hash + Send + Sync> InternalWorldServer<E> {
            #[doc(hidden)]
            pub fn diff_handler_global_count(&self) -> usize {
                self.shared.global_world_manager.read().global_diff_handler_count()
            }

            #[doc(hidden)]
            pub fn diff_handler_global_count_by_kind(
                &self,
            ) -> HashMap<naia_shared::ComponentKind, usize> {
                self.shared.global_world_manager.read().global_diff_handler_count_by_kind()
            }

            #[doc(hidden)]
            pub fn diff_handler_user_counts(&self) -> HashMap<UserKey, usize> {
                self.send.state.send_user_connections
                    .values()
                    .map(|c| (c.user_key, c.diff_handler_receiver_count()))
                    .collect()
            }

            #[doc(hidden)]
            pub fn scope_change_queue_len(&self) -> usize {
                self.shared.scope_change_queue.lock().len()
            }

            #[doc(hidden)]
            pub fn total_dirty_update_count(&self) -> usize {
                self.send.state.send_user_connections
                    .values()
                    .map(|c| c.base.world_manager.dirty_update_count())
                    .sum()
            }
        }
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::{LocalEntity, OwnedLocalEntity};

        // ── Shared `interior_visibility` read bodies (task-#9 `_impl` pattern) ──
        //
        // The user→local-entity converter is send-resident (it lives in each
        // user's `SendConnection.base.world_manager`). These free functions take
        // `&` refs to the coord-resident `user_store`, the send-resident
        // `send_user_connections`, and shared `global_entity_map`, so BOTH the
        // fused engine (resident) and the pipelined `&self` slot-lock read path
        // (`PipelinedWorldServer::{local_entities,local_to_world_entity,
        // world_to_local_entity}`) share ONE body — zero semantic drift across
        // the engine split. They mutate nothing. Mirrors
        // [`user_scope_has_entity_impl`].

        /// Panics if the user (or their send connection) does not exist — matches
        /// the resident `local_entities` contract.
        pub(crate) fn local_entities_impl(
            user_store: &UserStore,
            send_user_connections: &std::collections::HashMap<std::net::SocketAddr, crate::connection::SendConnection>,
            user_key: &UserKey,
        ) -> Vec<LocalEntity> {
            let user = user_store.get(user_key).expect("User does not exist");
            let send_conn = send_user_connections
                .get(&user.address())
                .expect("User connection does not exist");
            send_conn.base.world_manager.local_entities()
        }

        pub(crate) fn local_to_world_entity_impl<E: Copy + Eq + Hash + Send + Sync>(
            shared: &crate::server::ServerShared<E>,
            user_store: &UserStore,
            send_user_connections: &std::collections::HashMap<std::net::SocketAddr, crate::connection::SendConnection>,
            user_key: &UserKey,
            local_entity: &LocalEntity,
        ) -> Option<E> {
            let user = user_store.get(user_key)?;
            let send_conn = send_user_connections.get(&user.address())?;
            let converter = send_conn.base.world_manager.entity_converter();

            let owned_local_entity: OwnedLocalEntity = (*local_entity).into();
            let global_entity = converter
                .owned_entity_to_global_entity(&owned_local_entity)
                .ok()?;
            let world_entity = shared
                .global_entity_map
                .read()
                .global_entity_to_entity(&global_entity)
                .ok()?;

            Some(world_entity)
        }

        pub(crate) fn world_to_local_entity_impl<E: Copy + Eq + Hash + Send + Sync>(
            shared: &crate::server::ServerShared<E>,
            user_store: &UserStore,
            send_user_connections: &std::collections::HashMap<std::net::SocketAddr, crate::connection::SendConnection>,
            user_key: &UserKey,
            world_entity: &E,
        ) -> Option<LocalEntity> {
            let global_entity = shared
                .global_entity_map
                .read()
                .entity_to_global_entity(world_entity)
                .ok()?;

            let user = user_store.get(user_key)?;
            let send_conn = send_user_connections.get(&user.address())?;
            let converter = send_conn.base.world_manager.entity_converter();
            let owned_entity = converter
                .global_entity_to_owned_entity(&global_entity)
                .ok()?;

            Some(LocalEntity::from(owned_entity))
        }

        impl<E: Copy + Eq + Hash + Send + Sync> InternalWorldServer<E> {
            /// Returns all LocalEntity IDs for entities replicated to the given user.
            ///
            /// Returns the set of LocalEntity IDs that currently exist for that user
            /// (i.e., all entities replicated to that user).
            /// The ordering doesn't matter.
            ///
            /// # Panics
            ///
            /// Panics if the user does not exist.
            pub fn local_entities(&self, user_key: &UserKey) -> Vec<LocalEntity> {
                local_entities_impl(
                    &self.sim_handle.state.user_store,
                    &self.send.state.send_user_connections,
                    user_key,
                )
            }

            /// Retrieves an EntityRef that exposes read-only operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity<W: WorldRefType<E>>(
                &self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityRef<'_, E, W>> {
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity(world, &world_entity))
            }

            /// Retrieves an EntityMut that exposes read and write operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity_mut<W: WorldMutType<E>>(
                &mut self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityMut<'_, E, W>> {
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity_mut(world, &world_entity))
            }

            pub(crate) fn local_to_world_entity(
                &self,
                user_key: &UserKey,
                local_entity: &LocalEntity
            ) -> Option<E> {
                local_to_world_entity_impl(
                    &self.shared,
                    &self.sim_handle.state.user_store,
                    &self.send.state.send_user_connections,
                    user_key,
                    local_entity,
                )
            }

            pub(crate) fn world_to_local_entity(
                &self,
                user_key: &UserKey,
                world_entity: &E,
            ) -> Option<LocalEntity> {
                world_to_local_entity_impl(
                    &self.shared,
                    &self.sim_handle.state.user_store,
                    &self.send.state.send_user_connections,
                    user_key,
                    world_entity,
                )
            }
        }
    }
}

/// task #9 — the canonical `user_scope_has_entity` predicate, factored out of
/// [`InternalWorldServer::user_scope_has_entity`] so both the fused engine and
/// the pipelined `&self` slot-lock read path
/// ([`crate::PipelinedWorldServer::user_scope_has_entity_ref`]) share ONE body
/// — zero semantic drift across the engine split. Reads shared gwm/global-map
/// state plus the send-resident scope/room maps and the coord-resident
/// user/resource registries; mutates nothing.
pub(crate) fn user_scope_has_entity_impl<E: Copy + Eq + Hash + Send + Sync>(
    shared: &crate::server::ServerShared<E>,
    entity_scope_map: &EntityScopeMap,
    entity_room_map: &EntityRoomMap,
    user_store: &UserStore,
    resource_registry: &ResourceRegistry,
    user_key: &UserKey,
    world_entity: &E,
) -> bool {
    let global_entity = shared
        .global_entity_map
        .read()
        .entity_to_global_entity(world_entity)
        .unwrap();

    // Check if entity has Private replication config
    let is_private = if let Some(config) = shared
        .global_world_manager
        .read()
        .entity_replication_config(&global_entity)
    {
        matches!(config.publicity, Publicity::Private)
    } else {
        false
    };

    // Owning client is always in-scope for client-owned entities
    let is_owner = if let Some(
        EntityOwner::Client(owner_key)
        | EntityOwner::ClientWaiting(owner_key)
        | EntityOwner::ClientPublic(owner_key),
    ) = shared
        .global_world_manager
        .read()
        .entity_owner(&global_entity)
    {
        owner_key == *user_key
    } else {
        false
    };

    // If owner, always in scope
    if is_owner {
        return true;
    }

    // Per [entity-publication]: Private entities MUST NOT be in-scope for non-owners
    if is_private {
        return false;
    }

    // Check explicit include/exclude
    if let Some(in_scope) = entity_scope_map.get(user_key, &global_entity) {
        if *in_scope {
            // [entity-scopes-09]: explicit include() cannot bypass the room gate for
            // server-owned non-resource entities that have no rooms at all. Entities
            // in rooms (even rooms the user isn't in) are valid include() targets per
            // [entity-scopes-06]; only completely roomless entities are gated.
            let entity_is_roomless = entity_room_map.entity_get_rooms(&global_entity).is_none();
            if entity_is_roomless {
                let is_resource = resource_registry.is_resource_entity(&global_entity);
                let server_owned = shared
                    .global_world_manager
                    .read()
                    .entity_owner(&global_entity)
                    .map(|o| o.is_server())
                    .unwrap_or(false);
                if server_owned && !is_resource {
                    return false;
                }
            }
        }
        return *in_scope;
    }
    // Default: in-scope if user and entity share a room
    let Some(user) = user_store.get(user_key) else {
        return false;
    };
    let Some(entity_rooms) = entity_room_map.entity_get_rooms(&global_entity) else {
        return false;
    };
    let user_rooms = user.room_keys();
    entity_rooms.intersection(user_rooms).next().is_some()
}
