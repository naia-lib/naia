//! Cross-thread shared state for the C.3 three-stage pipeline.
//!
//! `ServerShared<E>` holds the `WorldServer` fields that are either
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
//! 1. connection_shared (RwLock<HashMap>)    — outermost
//! 2. global_world_manager.diff_handler()    — internal RwLock
//! 3. global_entity_map / idx_to_world       — RwLock
//! 4. time_manager                           — RwLock
//! 5. pending_send_state_updates             — Mutex
//! 6. scope_change_queue                     — Mutex
//! 7. pending_auth_grants                    — Mutex
//! ```
//!
//! Step 4-A introduces this discipline; subsequent steps (4-B onwards) add
//! the locked fields under this order.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use parking_lot::{Mutex, RwLock};

use naia_shared::{
    ChannelKinds, ComponentKinds, EntityAuthStatus, GlobalDirtyBitset, GlobalEntity,
    GlobalEntityMap, MessageKinds,
};

use crate::{
    server::{
        connection_shared::ConnectionShared, scope_change::ScopeChange,
        send_state_update::SendStateUpdate,
    },
    time_manager::TimeManager,
    ServerConfig, UserKey,
};

/// Cross-thread shared state for the three-stage pipeline.
///
/// All fields are either `Clone`-cheap immutable (config, kind tables) or
/// already internally thread-safe (`Arc<GlobalDirtyBitset>` uses atomics).
/// Wrapping the struct itself in `Arc<>` is therefore enough — no outer lock
/// is needed at this stage.
///
/// The `E` parameter mirrors `WorldServer<E>` so subsequent steps can add
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

    /// Recv → send handoff queue (step 4-E.2e). LOCK ORDER position #5.
    /// `finalize_connection` pushes `ConnectionAdded` here from the recv
    /// thread (which can't write to `SendState.send_user_connections`
    /// directly in pipeline mode). The disconnect path pushes
    /// `ConnectionRemoved`. Drained inline at `WorldServer::receive`'s
    /// tail in serial mode; drained by the coordinator at step 6.5 in
    /// pipeline mode. See `send_state_update.rs` for variant semantics.
    pub(crate) pending_send_state_updates: Mutex<Vec<SendStateUpdate<E>>>,

    /// Queue of scope-change events accumulated by coordinator code and
    /// drained at the top of `send_all_packets`. Mutex held briefly on
    /// push/drain; no hot-path contention.
    pub(crate) scope_change_queue: Mutex<VecDeque<ScopeChange>>,

    /// Auth grants deferred one tick to ensure entity registration on the
    /// client side. Drained at the end of `send_all_packets` Phase 3.
    pub(crate) pending_auth_grants:
        Mutex<Vec<(UserKey, GlobalEntity, EntityAuthStatus)>>,

    /// Per-connection `ConnectionShared` cells (atomics for ACK/RTT and
    /// coordinator → recv `should_disconnect` per B4). Outermost lock per
    /// the LOCK ORDER block above. Populated when a connection is finalized;
    /// removed when the user disconnects. `RecvConnection` and
    /// `SendConnection` will each hold a clone of the inner `Arc<>` once
    /// step 4-C lands the Connection split — the map itself stays here so
    /// the coordinator and the bevy event-application layer can address
    /// per-user atomics by `SocketAddr` without touching either state half.
    pub(crate) connection_shared: RwLock<HashMap<SocketAddr, Arc<ConnectionShared>>>,

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

    /// Dense `GlobalEntityIndex` → world-entity array (step 4-E.2c).
    /// Slot 0 (INVALID) is always `None`. Paired with `global_entity_map`
    /// at LOCK ORDER position #3 — they cover the same logical slot of
    /// information (one is HashMap-keyed, the other is dense-index keyed)
    /// and are always updated together.
    pub(crate) idx_to_world: RwLock<Vec<Option<E>>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> ServerShared<E> {
    /// Construct a new `ServerShared` from the components carved out of
    /// `WorldServer::new`.
    pub fn new(
        server_config: ServerConfig,
        channel_kinds: ChannelKinds,
        message_kinds: MessageKinds,
        component_kinds: ComponentKinds,
        client_authoritative_entities: bool,
        global_dirty: Arc<GlobalDirtyBitset>,
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
            pending_send_state_updates: Mutex::new(Vec::new()),
            scope_change_queue: Mutex::new(VecDeque::new()),
            pending_auth_grants: Mutex::new(Vec::new()),
            connection_shared: RwLock::new(HashMap::new()),
            time_manager: RwLock::new(TimeManager::new(tick_interval)),
            global_entity_map: RwLock::new(GlobalEntityMap::new()),
            idx_to_world: RwLock::new(vec![None; entity_index_capacity]),
        }
    }
}
