//! Coordinator-thread state owned by `WorldServer` (step 4-E.2a).
//!
//! Bundles the fields that are neither recv-thread-exclusive nor
//! send-thread-exclusive nor cross-thread-shared. Later 4-E.2 sub-commits
//! migrate individual fields out:
//!   * 4-E.2b — `time_manager` → `ServerShared` ✅ (landed)
//!   * 4-E.2c — `global_entity_map` + `idx_to_world` → `ServerShared` ✅ (landed)
//!   * 4-E.2d — `user_connections` dissolved into recv/send halves ✅ (landed)
//!   * 4-E.2e — `global_priority` split: authoritative read target moved
//!     to `SendState.global_priority`; `coord_state.global_priority_mirror`
//!     here is the borrow-API write target, cloned into `send` via
//!     publish-on-read at the top of `send_all_packets` ✅ (landed)
//!
//! Per the LOCK ORDER in `server_shared.rs`, no field on this struct is
//! locked — coordinator-thread access is single-threaded by definition.

use std::hash::Hash;

use naia_shared::{GlobalPriorityState, ResourceRegistry};

use crate::request::{GlobalRequestManager, GlobalResponseManager};

use super::{room_store::RoomStore, user_store::UserStore};

/// Coordinator-thread state lifted out of `WorldServer` (step 4-E.2a).
///
/// Fields marked with a `// → <step>` comment are migrated to a different
/// owner in a later sub-commit; their location here is purely transitional.
pub struct CoordinatorState<E: Copy + Eq + Hash + Send + Sync> {
    /// Per-user metadata (UserKey ↔ address mapping, disconnect tracking).
    pub(crate) user_store: UserStore,
    /// Per-room metadata.
    pub(crate) room_store: RoomStore,
    /// In-flight outbound requests awaiting matching responses.
    pub(crate) global_request_manager: GlobalRequestManager,
    /// In-flight outbound responses awaiting client receipt.
    pub(crate) global_response_manager: GlobalResponseManager,
    /// Sender-wide priority layer — *borrow-API target* (4-E.2e). The
    /// authoritative copy that Iris reads from lives at
    /// `SendState.global_priority` and is refreshed via `clone_from(&self
    /// .coord_state.global_priority_mirror)` at the top of every
    /// `send_all_packets`. Writes always go here first (through the
    /// `global_entity_priority_mut` borrow API or `on_despawn`); the
    /// publish-on-read step keeps `send.global_priority` in sync.
    pub(crate) global_priority_mirror: GlobalPriorityState<E>,
    /// Per-`TypeId<R>` ↔ `GlobalEntity` registry for Replicated Resources.
    pub(crate) resource_registry: ResourceRegistry,
    /// Optional lag-compensation snapshot buffer. `None` until enabled.
    pub(crate) historian: Option<crate::historian::Historian>,
}
