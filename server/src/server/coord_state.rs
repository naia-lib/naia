//! Coordinator-thread state owned by `WorldServer` (step 4-E.2a).
//!
//! Bundles the fields that are neither recv-thread-exclusive nor
//! send-thread-exclusive nor cross-thread-shared. Later 4-E.2 sub-commits
//! migrate individual fields out:
//!   * 4-E.2b — `time_manager` → `ServerShared` ✅ (landed)
//!   * 4-E.2c — `global_entity_map` + `idx_to_world` → `ServerShared` ✅ (landed)
//!   * 4-E.2d — `user_connections` dissolved into recv/send halves;
//!              `global_priority` → `SendState`
//!
//! Per the LOCK ORDER in `server_shared.rs`, no field on this struct is
//! locked — coordinator-thread access is single-threaded by definition.

use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use naia_shared::{GlobalPriorityState, ResourceRegistry};

use crate::{
    connection::connection::Connection,
    request::{GlobalRequestManager, GlobalResponseManager},
    world::{
        entity_room_map::EntityRoomMap, entity_scope_map::EntityScopeMap,
        global_world_manager::GlobalWorldManager,
    },
};

use super::{room_store::RoomStore, scope_checks_cache::ScopeChecksCache, user_store::UserStore};

/// Coordinator-thread state lifted out of `WorldServer` (step 4-E.2a).
///
/// Fields marked with a `// → <step>` comment are migrated to a different
/// owner in a later sub-commit; their location here is purely transitional.
pub struct CoordinatorState<E: Copy + Eq + Hash + Send + Sync> {
    /// Per-user metadata (UserKey ↔ address mapping, disconnect tracking).
    pub(crate) user_store: UserStore,
    /// Per-address full `Connection` wrappers — dissolved into recv/send
    /// halves in 4-E.2d.
    pub(crate) user_connections: HashMap<SocketAddr, Connection>,
    /// Per-room metadata.
    pub(crate) room_store: RoomStore,
    /// Entity ↔ room membership index.
    pub(crate) entity_room_map: EntityRoomMap,
    /// Entity ↔ per-user scope membership index.
    pub(crate) entity_scope_map: EntityScopeMap,
    /// Per-server-world replicated-entity registry.
    pub(crate) global_world_manager: GlobalWorldManager,
    /// In-flight outbound requests awaiting matching responses.
    pub(crate) global_request_manager: GlobalRequestManager,
    /// In-flight outbound responses awaiting client receipt.
    pub(crate) global_response_manager: GlobalResponseManager,
    /// Sender-wide priority layer. → SendState (4-E.2d).
    pub(crate) global_priority: GlobalPriorityState<E>,
    /// Push-based mirror of pending scope checks; reads are O(1).
    pub(crate) scope_checks_cache: ScopeChecksCache<E>,
    /// Per-`TypeId<R>` ↔ `GlobalEntity` registry for Replicated Resources.
    pub(crate) resource_registry: ResourceRegistry,
    /// Optional lag-compensation snapshot buffer. `None` until enabled.
    pub(crate) historian: Option<crate::historian::Historian>,
}

