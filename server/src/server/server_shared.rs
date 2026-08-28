//! Cross-thread shared state for the C.3 three-stage pipeline.
//!
//! `ServerShared<E>` holds the `InternalWorldServer` fields that are either
//! init-only-after-construction or already internally thread-safe. The
//! pipeline coordinator places this struct behind an `Arc<>` so the recv,
//! sim, and send threads can read it concurrently without contention.
//!
//! # LOCK ORDER (B11 — deadlock prevention)
//!
//! When future steps add `Mutex`/`RwLock`-protected fields to this struct,
//! any code that holds more than one such lock MUST acquire them in the
//! order below. Any inversion is a bug.
//!
//! ```text
//! 2. global_world_manager (outer RwLock)    — outermost
//! 2a. global_world_manager.diff_handler()   — internal RwLock (acquired AFTER the outer #2 guard)
//! 3. global_entity_map / idx_to_world       — RwLock
//! 4. time_manager                           — RwLock
//! 5. pending_send_state_updates             — Mutex
//! 6. scope_change_queue                     — Mutex
//! 7. pending_auth_grants                    — Mutex
//! 8. pending_outbound_packets               — Mutex (recv→send packet ferry)
//! 9. pending_handshakes                     — Mutex (recv→coordination handshake handoff)
//! 10. pending_world_hooks                    — Mutex (D.2.2 sim_handle→sim world-hook ferry)
//! 11. pending_disconnect_requests            — Mutex (D.3b.3 sim_handle→recv disconnect ferry)
//! ```
//!
//! Step 4-A introduces this discipline; subsequent steps (4-B onwards) add
//! the locked fields under this order.

use std::{collections::VecDeque, hash::Hash, net::SocketAddr, sync::Arc, time::Duration};

use parking_lot::{Mutex, RwLock};

use naia_shared::{
    AtomicBitSet, ChannelKinds, ComponentKinds, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, GlobalDirtyBitset, GlobalEntity, GlobalEntityMap, MessageKinds,
    OutgoingPacket,
};

use naia_shared::DisconnectReason;

use crate::{
    server::{
        configure_replication::ConfigureWorldOp, scope_change::ScopeChange,
        send_state_update::SendStateUpdate,
    },
    time_manager::TimeManager,
    world::global_world_manager::GlobalWorldManager,
    ServerConfig, UserKey,
};

/// Cross-thread shared state for the three-stage pipeline.
///
/// All fields are either `Clone`-cheap immutable (config, kind tables) or
/// already internally thread-safe (`Arc<GlobalDirtyBitset>` uses atomics).
/// Wrapping the struct itself in `Arc<>` is therefore enough — no outer lock
/// is needed at this stage.
///
/// The `E` parameter mirrors `InternalWorldServer<E>` so subsequent steps can add
/// `E`-generic fields (e.g. `global_entity_map: RwLock<GlobalEntityMap<E>>`)
/// without changing this signature.
pub struct ServerShared<E: Copy + Eq + Hash + Send + Sync> {
    /// Server configuration — set at construction, never mutated.
    pub server_config: ServerConfig,

    /// Channel kind registry — set at construction, never mutated.
    pub channel_kinds: ChannelKinds,

    /// Message kind registry — set at construction, never mutated.
    pub message_kinds: MessageKinds,

    /// Component kind registry — set at construction, never mutated.
    pub component_kinds: ComponentKinds,

    /// Whether clients are allowed to author entities — set at construction.
    pub client_authoritative_entities: bool,

    /// Global dirty bitset — already atomic; recv writes, send reads.
    pub global_dirty: Arc<GlobalDirtyBitset>,

    /// MISSION_SNAPSHOT_DIRTY_TRIM (2026-05-20) — cross-thread
    /// "needed entity" set: entities with an in-flight value-reading
    /// reliable command (Spawn / SpawnWithComponents / InsertComponent)
    /// to ANY user, keyed by `GlobalEntityIndex`. Recomputed each tick by
    /// `SendState::refresh_needed_entities` (send-side, inside the park
    /// window) and read by `SendStateView::needed_snapshot_entries`
    /// (Sim-side snapshot build, also inside the park window — so writer
    /// and reader never overlap; the bitset is atomic only to satisfy the
    /// `Sync` bound).
    ///
    /// The complete "what the snapshot must contain" set is this set
    /// UNION `global_dirty` (the latter covers component value updates +
    /// drop-reflagged re-sends). See `MISSION_SNAPSHOT_DIRTY_TRIM.md` §4.
    pub needed_entities: Arc<AtomicBitSet>,

    /// Recv → send handoff queue (step 4-E.2e). LOCK ORDER position #5.
    /// `finalize_connection` pushes `ConnectionAdded` here from the recv
    /// thread (which can't write to `SendState.send_user_connections`
    /// directly in pipeline mode). The disconnect path pushes
    /// `ConnectionRemoved`. Drained inline at `InternalWorldServer::receive`'s
    /// tail in serial mode; drained by the coordinator at step 6.5 in
    /// pipeline mode. See `send_state_update.rs` for variant semantics.
    pub(crate) pending_send_state_updates: Mutex<Vec<SendStateUpdate>>,

    /// Queue of scope-change events accumulated by coordinator code and
    /// drained at the top of `send_all_packets`. Mutex held briefly on
    /// push/drain; no hot-path contention.
    pub(crate) scope_change_queue: Mutex<VecDeque<ScopeChange<E>>>,

    /// Auth grants deferred one tick to ensure entity registration on the
    /// client side. Drained at the end of `send_all_packets` Phase 3.
    pub(crate) pending_auth_grants: Mutex<Vec<(UserKey, GlobalEntity, EntityAuthStatus)>>,

    /// 4-F.naia.c.1: outbound packets queued by the recv path because the
    /// recv thread doesn't own `SendState::send_io`. Used for Handshake
    /// Connect Responses and Ping Pong responses. Drained at the top of
    /// `send_all_packets` via `flush_pending_outbound_packets`. LOCK ORDER
    /// position #8 (after `pending_auth_grants`, before `pending_handshakes`).
    /// Briefly-held Mutex on push/drain — no hot-path contention.
    pub(crate) pending_outbound_packets: Mutex<Vec<(SocketAddr, OutgoingPacket)>>,

    /// 4-F.naia.c.1: addresses for which the recv path observed a Handshake
    /// `ClientConnectRequest` packet. Recv pushes (one entry per inbound
    /// handshake — `take_disconnected` is idempotent on the drain side per
    /// the spec's Option C-2). Drained by the coordinator-stage
    /// `drain_pending_handshakes`, which looks up the matching user_key
    /// via `sim_handle.user_store.take_disconnected` and calls
    /// `finalize_connection`. LOCK ORDER position #9 (last).
    pub(crate) pending_handshakes: Mutex<Vec<SocketAddr>>,

    /// Server tick clock + tick-duration EWMA (step 4-E.2b). LOCK ORDER
    /// position #4. The recv thread takes the **write** guard once per
    /// tick inside `take_tick_events` (calling `recv_server_tick`); every
    /// other site — `process_ping`, `current_tick`, ping/heartbeat send
    /// paths, the send-loop Iris phase — takes a brief **read** guard.
    pub(crate) time_manager: RwLock<TimeManager>,

    /// World-entity ↔ GlobalEntity bidirectional map (step 4-E.2c).
    /// LOCK ORDER position #3 (paired with `idx_to_world`). The
    /// coordinator takes **write** for spawn / despawn / reservation
    /// flows; every other path (send Iris phase, EntityAndGlobalEntity-
    /// Converter impl, room/scope plumbing) takes a brief **read** guard.
    /// Hot send-side loops hold one read guard for the whole tick scope
    /// to amortize the RwLock acquisition.
    pub(crate) global_entity_map: RwLock<GlobalEntityMap<E>>,

    /// Per-world replicated-entity registry (step 4-F.naia.a).
    /// LOCK ORDER position #2 (outer wrapper). The inner
    /// `diff_handler: Arc<RwLock<GlobalDiffHandler>>` keeps its own
    /// internal RwLock at LOCK ORDER position #2a — acquire AFTER the
    /// outer #2 guard, never inverted. Coordinator-thread paths take
    /// **write** for spawn / despawn / publication / delegation /
    /// authority transitions; the send thread takes **read** for the
    /// Iris loop (one guard amortized across the whole tick); the recv
    /// thread is read-only for connection lifecycle queries.
    pub(crate) global_world_manager: RwLock<GlobalWorldManager>,

    /// Dense `GlobalEntityIndex` → world-entity array (step 4-E.2c).
    /// Slot 0 (INVALID) is always `None`. Paired with `global_entity_map`
    /// at LOCK ORDER position #3 — they cover the same logical slot of
    /// information (one is HashMap-keyed, the other is dense-index keyed)
    /// and are always updated together.
    pub(crate) idx_to_world: RwLock<Vec<Option<E>>>,

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — pending
    /// World-side `configure_entity_replication` hook ops, pushed by
    /// `CoordHandle::configure_entity_replication` and drained by
    /// `SendHandle::apply_pending_world_hooks<W>` on the Sim system
    /// (the only stage holding the `&mut World`). LOCK ORDER position
    /// #10 (last) — briefly-held Mutex on push/drain, no hot-path
    /// contention. Kept separate from `scope_change_queue` because the
    /// drain site differs (world parameter required).
    pub(crate) pending_world_hooks: Mutex<VecDeque<ConfigureWorldOp<E>>>,

    /// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — pending
    /// disconnect requests pushed by `CoordHandle::disconnect_user` and
    /// drained by the recv path at the top of `process_disconnects`,
    /// immediately before `outstanding_disconnects` is consumed. LOCK
    /// ORDER position #11 (last) — briefly-held Mutex on push/drain, no
    /// hot-path contention.
    pub(crate) pending_disconnect_requests: Mutex<Vec<(UserKey, DisconnectReason)>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> ServerShared<E> {
    /// Construct a new `ServerShared` from the components carved out of
    /// `InternalWorldServer::new`.
    // The argument list mirrors the fields carved out of
    // `InternalWorldServer::new`; grouping them into a params struct would
    // just move the same list one level out.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server_config: ServerConfig,
        channel_kinds: ChannelKinds,
        message_kinds: MessageKinds,
        component_kinds: ComponentKinds,
        client_authoritative_entities: bool,
        global_dirty: Arc<GlobalDirtyBitset>,
        global_world_manager: GlobalWorldManager,
        tick_interval: Duration,
        entity_index_capacity: usize,
    ) -> Self {
        Self {
            server_config,
            channel_kinds,
            message_kinds,
            component_kinds,
            client_authoritative_entities,
            global_dirty,
            // One bit per GlobalEntityIndex (slot 0 = INVALID sentinel).
            needed_entities: Arc::new(AtomicBitSet::new(entity_index_capacity)),
            pending_send_state_updates: Mutex::new(Vec::new()),
            scope_change_queue: Mutex::new(VecDeque::new()),
            pending_auth_grants: Mutex::new(Vec::new()),
            pending_outbound_packets: Mutex::new(Vec::new()),
            pending_handshakes: Mutex::new(Vec::new()),
            time_manager: RwLock::new(TimeManager::new(tick_interval)),
            global_world_manager: RwLock::new(global_world_manager),
            global_entity_map: RwLock::new(GlobalEntityMap::new()),
            idx_to_world: RwLock::new(vec![None; entity_index_capacity]),
            pending_world_hooks: Mutex::new(VecDeque::new()),
            pending_disconnect_requests: Mutex::new(Vec::new()),
        }
    }
}

// MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19) — see
// `pipeline_actors/sim_converter.rs` for rationale + wire-identity
// argument. Mirrors `InternalWorldServer<E>`'s impl at
// `server/src/server/world_server.rs:3956`; both delegate to the same
// `global_entity_map` `RwLock` so `EntityProperty::set` produces
// byte-identical wire payloads.
impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E> for ServerShared<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.global_entity_map
            .read()
            .global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
    }
}
