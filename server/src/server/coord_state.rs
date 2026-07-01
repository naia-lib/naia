//! Coordinator-thread state owned by `InternalWorldServer` (step 4-E.2a).
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

use std::{collections::HashMap, hash::Hash};

use naia_shared::{
    ComponentKind, GlobalEntity, GlobalEntityIndex, GlobalPriorityState, ResourceRegistry,
    UserPriorityState,
};

use crate::request::{GlobalRequestManager, GlobalResponseManager};
use crate::user::UserKey;

use super::{room_store::RoomStore, user_store::UserStore};

/// Coordinator-thread state lifted out of `InternalWorldServer` (step 4-E.2a).
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
    /// task #13 — per-user priority **staging** (the per-tick borrow-API write
    /// target on the pipelined coord side). Unlike `global_priority_mirror`
    /// (which persists + republishes via `clone_from`), this is DRAINED+CLEARED
    /// into `SendState.user_priorities` every `send` (`drain_merge_into`),
    /// because the per-user layer carries the LIVE accumulator that lives
    /// send-side and must not be clobbered. Clearing each tick is what gives
    /// eviction parity with the resident direct-write path for free. `None`
    /// entries are never created; absent ⇒ no pending per-user writes.
    pub(crate) user_priority_staging: HashMap<UserKey, UserPriorityState<E>>,
    /// Phase C / D7 — pending explicit user-scope mutations authored on coord
    /// and published into `SendState.entity_scope_map` immediately before D8
    /// send-prep. This mirrors `user_priority_staging`: public APIs can enqueue
    /// without reassembling the split engine, while the D-slot preserves the
    /// byte-critical send order.
    pub(crate) pending_scope_ledger_ops: Vec<PendingScopeLedgerOp<E>>,
    /// Phase C / D2 — pending replicated-resource send-side publication and
    /// cleanup authored by the coord/world half of `insert_resource` and
    /// `remove_resource`.
    pub(crate) pending_resource_ops: Vec<PendingResourceOp<E>>,
    /// Phase C / D3 — pending lifecycle send-side mutations authored by coord
    /// APIs after applying their coord/global effects synchronously.
    pub(crate) pending_lifecycle_ops: Vec<PendingLifecycleOp<E>>,
    /// Per-`TypeId<R>` ↔ `GlobalEntity` registry for Replicated Resources.
    pub(crate) resource_registry: ResourceRegistry,
    /// Optional lag-compensation snapshot buffer. `None` until enabled.
    pub(crate) historian: Option<crate::historian::Historian>,
}

/// Phase C / D7 — concrete scope-ledger mutations staged on coord.
pub(crate) enum PendingScopeLedgerOp<E> {
    Set {
        user_key: UserKey,
        world_entity: E,
        is_contained: bool,
    },
    RemoveUser {
        user_key: UserKey,
    },
}

/// Phase C / D2 — concrete resource mutations staged on coord.
pub(crate) enum PendingResourceOp<E> {
    AutoScopeUsers {
        user_keys: Vec<UserKey>,
        world_entity: E,
    },
    Remove {
        world_entity: E,
        global_entity: GlobalEntity,
        entity_idx: GlobalEntityIndex,
    },
}

/// Phase C / D3 — concrete lifecycle mutations staged on coord.
pub(crate) enum PendingLifecycleOp<E> {
    InsertComponent {
        global_entity: GlobalEntity,
        component_kind: ComponentKind,
    },
    RemoveComponent {
        global_entity: GlobalEntity,
        component_kind: ComponentKind,
    },
    DespawnEntity {
        world_entity: E,
        global_entity: GlobalEntity,
        entity_idx: GlobalEntityIndex,
    },
}
