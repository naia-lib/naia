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
    collections::VecDeque,
    hash::Hash,
    marker::PhantomData,
    sync::Arc,
};

use parking_lot::Mutex;

use naia_shared::{
    ChannelKinds, ComponentKinds, EntityAuthStatus, GlobalDirtyBitset, GlobalEntity, MessageKinds,
};

use crate::{server::scope_change::ScopeChange, ServerConfig, UserKey};

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

    /// Queue of scope-change events accumulated by coordinator code and
    /// drained at the top of `send_all_packets`. Mutex held briefly on
    /// push/drain; no hot-path contention.
    pub(crate) scope_change_queue: Mutex<VecDeque<ScopeChange>>,

    /// Auth grants deferred one tick to ensure entity registration on the
    /// client side. Drained at the end of `send_all_packets` Phase 3.
    pub(crate) pending_auth_grants:
        Mutex<Vec<(UserKey, GlobalEntity, EntityAuthStatus)>>,

    _phantom: PhantomData<E>,
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
    ) -> Self {
        Self {
            server_config,
            channel_kinds,
            message_kinds,
            component_kinds,
            client_authoritative_entities,
            global_dirty,
            scope_change_queue: Mutex::new(VecDeque::new()),
            pending_auth_grants: Mutex::new(Vec::new()),
            _phantom: PhantomData,
        }
    }
}
