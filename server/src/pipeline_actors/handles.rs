//! `CoordHandle<E>` — bundles the coordination state with a clone
//! of the cross-thread shared init (`Arc<ServerShared<E>>`) so cyberlith's
//! Sim app can install it as a single resource.
//!
//! Naming (MISSION_USER_ONLY_SEES_SIM Phase H): there is no separate
//! coordinator thread — the coordination capability is folded into the
//! Sim/main thread, so this handle is named `CoordHandle`. The underlying
//! per-thread state struct keeps the name [`CoordinatorState`] because it
//! denotes the *coordination-state capability* (the third of the
//! recv-side / send-side / coordination-side split), distinct from the
//! handle that carries it.
//!
//! The `Arc<ServerShared<E>>` clone here is independent of (but identical
//! to) the clones living on `RecvHandle::state.shared` and
//! `SendHandle::state.shared` — all three point at the same underlying
//! [`ServerShared`] allocation.

use std::{collections::hash_set::Iter, hash::Hash, net::SocketAddr, sync::Arc};

use std::time::Duration;

use naia_shared::{
    DisconnectReason, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError,
    EntityPriorityMut, EntityPriorityRef, GlobalEntity, GlobalEntitySpawner, ReplicatedComponent,
    Tick,
};

use crate::room::{Room, RoomKey};
use crate::server::coord_state::CoordinatorState;
use crate::server::scope_change::ScopeChange;
use crate::server::ServerShared;
use crate::user::{UserKey, WorldUser};
use crate::EntityOwner;

/// Coordination-side half of a pipeline-mode [`crate::InternalWorldServer`],
/// held by the Sim/main thread.
///
/// Holds [`CoordinatorState`] (`user_store`, `room_store`, scope tables,
/// historian, etc.) plus a clone of the cross-thread `Arc<ServerShared>`
/// so the coordination-side code can read shared init (`server_config`,
/// kind tables, `global_dirty`) without going through the recv or send
/// handle.
pub struct CoordHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Coordination-side state, owned by the Sim/main thread.
    /// Single-threaded — no internal locking.
    pub state: CoordinatorState<E>,
    /// Cross-thread shared init + atomic cells. Cloneable `Arc` — the
    /// same allocation is also referenced by [`crate::RecvHandle::state.shared`]
    /// and [`crate::SendHandle::state.shared`].
    pub shared: Arc<ServerShared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> CoordHandle<E> {
    // ============================================================
    // Phase B.7 — coordination-side read API surface for the bevy adapter
    // pipeline bridge (`apply_receive_output_pipeline`).
    // ============================================================
    //
    // All methods below are pure coordination-state reads — no recv/send
    // mutation. They mirror the namesake methods on `InternalWorldServer`.

    /// O(1): is `world_entity` a hidden resource entity? Used by the
    /// pipeline event-emission filter to suppress Spawn/Despawn events
    /// for resource entities.
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return false;
        };
        self.state
            .resource_registry
            .is_resource_entity(&global_entity)
    }

    /// Returns the current [`EntityOwner`] — who holds authoritative
    /// control over this entity right now. Mirrors
    /// `InternalWorldServer::entity_owner` (which reads only from `shared`).
    ///
    /// The pre-B.7 bevy adapter passed a world reference here for
    /// symmetry with `InternalWorldServer::entity(world, entity).owner()`, but
    /// the underlying body never reads from world — only from
    /// `shared.global_entity_map` + `shared.global_world_manager`. The
    /// world parameter is omitted here.
    pub fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return EntityOwner::Local;
        };
        if let Some(owner) = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(&global_entity)
        {
            return owner;
        }
        EntityOwner::Local
    }

    /// Returns whether a User exists for the given UserKey.
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.state.user_store.contains(user_key)
    }

    /// Returns the list of currently-registered user keys.
    ///
    /// Note: this returns ALL registered users (including those whose
    /// handshake has not yet completed). For the handshaked-only set,
    /// use `InternalWorldServer::user_keys` via `run_with_world_server`, which
    /// filters by `send_user_connections` membership.
    pub fn user_keys(&self) -> Vec<UserKey> {
        self.state.user_store.keys_copied()
    }

    /// Returns the address of the user with the given key, or `None`
    /// if the key is stale.
    pub fn user_address(&self, user_key: &UserKey) -> Option<std::net::SocketAddr> {
        self.state.user_store.address(user_key)
    }

    /// Returns the current server tick (read from the shared time
    /// manager).
    pub fn current_tick(&self) -> Tick {
        self.shared.time_manager.read().current_tick()
    }

    /// Returns a read-only reference to the coord-resident Historian, or
    /// `None` if lag-compensation snapshotting has not been enabled.
    pub fn historian(&self) -> Option<&crate::historian::Historian> {
        self.state.historian.as_ref()
    }

    /// Average tick duration (read from the shared time manager).
    pub fn average_tick_duration(&self) -> Duration {
        self.shared.time_manager.read().average_tick_duration()
    }

    /// Total number of replicated entities.
    pub fn entity_count(&self) -> usize {
        self.shared.global_entity_map.read().entity_count()
    }

    /// Number of users currently tracked.
    pub fn users_count(&self) -> usize {
        self.state.user_store.len()
    }

    /// Number of fully-connected users.
    pub fn user_count(&self) -> usize {
        self.user_keys().len()
    }

    /// Whether a room exists for the given key.
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.state.room_store.contains(room_key)
    }

    /// Keys of all rooms.
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.state.room_store.keys()
    }

    /// Number of rooms.
    pub fn rooms_count(&self) -> usize {
        self.state.room_store.len()
    }

    /// Number of rooms (alias).
    pub fn room_count(&self) -> usize {
        self.room_keys().len()
    }

    /// True iff a resource of type `R` is currently inserted.
    pub fn has_resource<R: ReplicatedComponent>(&self) -> bool {
        self.state.resource_registry.entity_for::<R>().is_some()
    }

    /// The hidden entity carrying resource `R`, or `None`.
    pub fn resource_entity<R: ReplicatedComponent>(&self) -> Option<E> {
        let global_entity = self.state.resource_registry.entity_for::<R>()?;
        self.shared
            .global_entity_map
            .read()
            .global_entity_to_entity(&global_entity)
            .ok()
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        self.state.resource_registry.len()
    }

    /// Server-side authority status for resource `R`, or `None`.
    pub fn resource_authority_status<R: ReplicatedComponent>(&self) -> Option<EntityAuthStatus> {
        let entity = self.resource_entity::<R>()?;
        self.entity_authority_status(&entity)
    }

    /// Convert a [`GlobalEntity`] to the consumer's entity handle.
    pub fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.shared
            .global_entity_map
            .read()
            .global_entity_to_entity(global_entity)
    }

    /// Convert the consumer's entity handle to a [`GlobalEntity`].
    pub fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19) — return a
    /// cloneable [`crate::pipeline_actors::ServerEntityConverter`] view over
    /// this server's `Arc<ServerShared<E>>`.
    ///
    /// Cyberlith installs the returned `ServerEntityConverter` as a Bevy
    /// `Resource` on the Sim app so Sim systems can construct
    /// `EntityProperty`-bearing messages or components without
    /// reassembling a `InternalWorldServer<E>`. The backing
    /// `EntityAndGlobalEntityConverter<E>` impl on `ServerShared<E>`
    /// delegates to the same `global_entity_map` `RwLock` that
    /// `InternalWorldServer`'s converter reads, so wire output is byte-identical.
    pub fn entity_converter(&self) -> crate::pipeline_actors::ServerEntityConverter<E>
    where
        E: 'static,
    {
        let shared_clone: Arc<crate::server::ServerShared<E>> = Arc::clone(&self.shared);
        let arc: Arc<dyn EntityAndGlobalEntityConverter<E> + Send + Sync> = shared_clone;
        crate::pipeline_actors::ServerEntityConverter::from_arc(arc)
    }

    // ====================================================================
    // MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — Coord-only
    // connection lifecycle: receive_user / disconnect_user.
    // ====================================================================
    //
    // `receive_user`: mirrors `InternalWorldServer::receive_user` (world_server.rs:270)
    // field-for-field. Both operations touch only `self.state.user_store`.
    //
    // `disconnect_user`: the underlying `user_queue_disconnect` (world_server.rs:3006)
    // touches recv-only state (`recv_user_connections`, `outstanding_disconnects`).
    // It cannot run from the Sim/main thread. Instead, we push the request to
    // `shared.pending_disconnect_requests` (LOCK ORDER #11). The recv path
    // drains it at the top of `process_disconnects`, immediately before
    // `outstanding_disconnects` is consumed, which preserves atomicity within
    // one recv tick.

    /// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — register a
    /// newly-accepted user so the coordination state can track their scope without
    /// reassembling a `InternalWorldServer`.
    ///
    /// Byte-identical to `InternalWorldServer::receive_user`: inserts a `WorldUser`
    /// record into the user_store and registers the address in the
    /// disconnected-users map so the subsequent handshake can look it up via
    /// `user_store.take_disconnected`.
    pub fn receive_user(&mut self, user_key: UserKey, user_addr: SocketAddr) {
        self.state
            .user_store
            .insert(user_key, WorldUser::new(user_addr));
        self.state
            .user_store
            .register_disconnected(user_addr, user_key);
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.3b.3 (2026-05-19) — request a
    /// user disconnect without reassembling a `InternalWorldServer`.
    ///
    /// Idempotent: returns silently if the user is not in the user_store
    /// (already disconnected or never registered). Otherwise pushes
    /// `(user_key, DisconnectReason::Kicked)` onto
    /// `shared.pending_disconnect_requests`; the recv path drains the queue
    /// at the top of `process_disconnects` via `user_queue_disconnect`.
    pub fn disconnect_user(&mut self, user_key: &UserKey) {
        if !self.state.user_store.contains(user_key) {
            return;
        }
        self.shared
            .pending_disconnect_requests
            .lock()
            .push((*user_key, DisconnectReason::Kicked));
    }

    // ====================================================================
    // Room-ops API (flat methods; push to scope_change_queue, no drain).
    // Send's apply_pending_room_changes drains on next send_all_packets.
    // ====================================================================

    /// Create a new room and return its key.
    pub fn create_room(&mut self) -> RoomKey {
        self.state.room_store.insert(Room::new())
    }

    /// Destroy a room. Returns true if the room existed.
    pub fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        let (existed, room_change_opt) = {
            let entity_map = self.shared.global_entity_map.read();
            self.state
                .room_store
                .destroy(room_key, &mut self.state.user_store, &*entity_map)
        };
        if let Some(room_change) = room_change_opt {
            self.shared
                .scope_change_queue
                .lock()
                .push_back(ScopeChange::RoomChange(room_change));
        }
        existed
    }

    /// Add a user to a room. Push-only; Send drains on next tick.
    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        let (legacy_change, room_change) = {
            let entity_map = self.shared.global_entity_map.read();
            self.state.room_store.add_user(
                room_key,
                user_key,
                &mut self.state.user_store,
                &*entity_map,
            )
        };
        let mut q = self.shared.scope_change_queue.lock();
        q.push_back(legacy_change);
        q.push_back(ScopeChange::RoomChange(room_change));
    }

    /// Remove a user from a room. Push-only; Send drains on next tick.
    pub fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        let (legacy_change, room_change) =
            self.state
                .room_store
                .remove_user::<E>(room_key, user_key, &mut self.state.user_store);
        let mut q = self.shared.scope_change_queue.lock();
        q.push_back(legacy_change);
        q.push_back(ScopeChange::RoomChange(room_change));
    }

    /// Add an entity to a room. Push-only; Send drains on next tick.
    pub fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.state
                .room_store
                .add_entity(room_key, world_entity, &*entity_map)
        };
        if let Some((legacy_change, room_change)) = pair_opt {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
    }

    /// C.6 prep — number of pending `ScopeChange` entries on the
    /// cross-half `scope_change_queue`. Used by Send's preamble drain
    /// tests + cyberlith Send SubApp telemetry to observe backpressure
    /// between the coordination/cyberlith-Sim room mutations and the Send-side
    /// drain. O(1) lock + len().
    pub fn scope_change_queue_len(&self) -> usize {
        self.shared.scope_change_queue.lock().len()
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — number of
    /// pending World-side `configure_entity_replication` hook ops on the
    /// `pending_world_hooks` queue. Used by tests + cyberlith Sim
    /// telemetry to observe that world-hook registration is deferred to
    /// (and consumed by) `apply_pending_world_hooks`. O(1) lock + len().
    pub fn pending_world_hooks_len(&self) -> usize {
        self.shared.pending_world_hooks.lock().len()
    }

    /// Remove an entity from a room. Push-only; Send drains on next tick.
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.state
                .room_store
                .remove_entity(room_key, world_entity, &*entity_map)
        };
        if let Some((legacy_change, room_change)) = pair_opt {
            let mut q = self.shared.scope_change_queue.lock();
            q.push_back(legacy_change);
            q.push_back(ScopeChange::RoomChange(room_change));
        }
    }

    // ====================================================================
    // MISSION_USER_ONLY_SEES_SIM Phase D.2.1 (2026-05-19) — Coord-only
    // entity-replication enablement.
    // ====================================================================
    //
    // `InternalWorldServer::enable_entity_replication` (`world_server.rs:1044`)
    // delegates to `spawn_entity_inner` which exclusively writes Coord-
    // side shared state: `shared.global_entity_map`,
    // `shared.global_world_manager`, and `shared.idx_to_world`. There is
    // NO Send-side state mutation and NO world-hook registration on this
    // path — `EntityOwner::Server` registration alone does not install
    // per-component diff mutators (those land at `entity_publish` time,
    // which is a separate `configure_entity_replication` concern handled
    // by D.2.2).
    //
    // End-to-end audit (`feedback-verify-before-proposing`) found that
    // the spec's prescribed `ScopeChange::EnableReplication` variant +
    // `apply_pending_world_hooks` registration would have nothing to
    // carry — there is no deferrable Send-side work to defer. Exposing
    // the existing shared-state writes as a Coord-only method is
    // therefore the entire delta. Backward compat:
    // `InternalWorldServer::enable_entity_replication` remains unchanged.

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.1 (2026-05-19) —
    /// register `entity` as a server-owned replicating entity without
    /// reassembling a `InternalWorldServer`.
    ///
    /// Byte-identical to `InternalWorldServer::enable_entity_replication`:
    /// writes the same three shared-state fields
    /// (`global_entity_map`, `global_world_manager`, `idx_to_world`)
    /// in the same order. No Send-side mutation; no world-hook
    /// registration; no deferred drainer work.
    ///
    /// Replaces the `run_with_world_server(sim_handle, recv, send, |ws|
    /// ws.enable_entity_replication(&entity))` reassembly pattern that
    /// cyberlith's `server_access::drain_sim_registrations` and
    /// `drain_sim_tile_registrations` use today.
    pub fn enable_entity_replication(&mut self, world_entity: &E) {
        // Fail-loud on double-enable: enable_entity_replication mirrors
        // spawn_entity_inner and is "enable once per entity." A second call
        // would silently clobber the GlobalEntity mapping (GlobalEntityMap::
        // spawn unconditionally mints a new GlobalEntity for local spawns),
        // orphaning every connection that tracks the old one.
        if self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
            .is_ok()
        {
            log::warn!(
                "enable_entity_replication called on already-registered entity — \
                 each entity may only be enabled once; ignoring"
            );
            debug_assert!(
                false,
                "enable_entity_replication double-enable: entity already in global_entity_map"
            );
            return;
        }
        // Mirror `InternalWorldServer::spawn_entity_inner` field-for-field —
        // identical write order, identical inputs, same locks.
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
            self.shared.set_idx_to_world(idx, Some(*world_entity));
        }
    }

    // ====================================================================
    // MISSION_USER_ONLY_SEES_SIM Phase D.3b.1 (2026-05-19) — Coord-only
    // `mark_entity_as_static`.
    // ====================================================================
    //
    // End-to-end audit (`feedback-verify-before-proposing`) of
    // `InternalWorldServer::mark_entity_as_static` (`world_server.rs:1133`) ->
    // `GlobalWorldManager::mark_entity_as_static`
    // (`global_world_manager.rs:104`): the entire operation is a single
    // gwm flag write — `record.is_static = true`. There is NO Send-side
    // state-machine mutation and NO world-hook registration; the
    // `is_static` flag is read only by Coord-side diff machinery during
    // packet assembly. It is therefore a trivial Coord-only mirror like
    // `enable_entity_replication` above — no `ScopeChange` variant and no
    // deferred drainer work (the D.2.2 deferred pattern would have nothing
    // to carry).
    //
    // Mirrors the legacy resolve-then-write sequence field-for-field,
    // including the same panic when the entity is absent from the global
    // map. Backward compat: `InternalWorldServer::mark_entity_as_static` is
    // unchanged. This retires the last `run_with_world_server`
    // reassembly in cyberlith's `drain_sim_tile_registrations`
    // (Phase E.6).

    /// MISSION_USER_ONLY_SEES_SIM Phase D.3b.1 (2026-05-19) — mark
    /// `world_entity` as static (its component data is not re-sent after
    /// the initial spawn packet) without reassembling a `InternalWorldServer`.
    ///
    /// Coord-side-only: writes the `is_static` flag on the entity's
    /// `global_world_manager` record. No Send-side mutation; no world-hook
    /// registration; no deferred drainer work. Field-for-field identical
    /// to `InternalWorldServer::mark_entity_as_static`, including the panic when
    /// the entity is not yet in the global map.
    pub fn mark_entity_as_static(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            panic!("entity not found in global map");
        };
        self.shared
            .global_world_manager
            .write()
            .mark_entity_as_static(&global_entity);
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.3b.2 (2026-05-19) — pure
    /// Coord-side read of the entity's authority status. Mirrors
    /// `InternalWorldServer::entity_authority_status` (`world_server.rs:1698`) —
    /// reads only `shared.global_entity_map` + `shared.global_world_manager`.
    /// Used by the host-sync drain's auth gate (skip insert/remove/despawn
    /// when a client holds authority). Returns `None` when the entity is
    /// not in the global map.
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        let global_entity = match self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        {
            Ok(ge) => ge,
            Err(_) => return None,
        };
        self.shared
            .global_world_manager
            .read()
            .entity_authority_status(&global_entity)
    }

    /// Returns the [`ReplicationConfig`] for `world_entity`, or `None` if it is
    /// not registered. Coord-only read (mirrors
    /// `InternalWorldServer::entity_replication_config`, reads only `shared`);
    /// returns `None` for an unknown entity rather than panicking (the safe
    /// fallback shared with the other `CoordHandle` reads).
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<crate::ReplicationConfig> {
        let Ok(global_entity) = self
            .shared
            .global_entity_map
            .read()
            .entity_to_global_entity(world_entity)
        else {
            return None;
        };
        self.shared
            .global_world_manager
            .read()
            .entity_replication_config(&global_entity)
    }

    /// Returns `true` if `world_entity` has been marked as static. Mirrors
    /// `InternalWorldServer::entity_is_static` (reads only from `shared`).
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        use naia_shared::GlobalWorldManagerType;
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
            .entity_is_static(&global_entity)
    }

    // ====================================================================
    // MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — Coord-only
    // entity-replication reconfiguration with deferred Send/World work.
    // ====================================================================
    //
    // Byte-identity strategy (vs `InternalWorldServer::configure_entity_replication`,
    // `world_server.rs:1423`): run the publicity-transition decision tree
    // EXACTLY ONCE here, reading PRE-transition gwm state, performing the
    // Coord-side gwm writes immediately (so a subsequent same-tick
    // configure on the same entity reads the post-transition state, as it
    // would under the fused path), and recording the Send-side and
    // World-side leaf work as flat ordered op lists. The tree is NOT
    // re-evaluated in the drain — that would mis-branch off the
    // already-transitioned gwm. Capturing a flat op list with all
    // per-connection parameters resolved at push time is the B.2
    // blocker-1 read-before-write fix (see `configure_replication.rs`).

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — set the
    /// [`ReplicationConfig`](crate::ReplicationConfig) for `world_entity`
    /// without reassembling a `InternalWorldServer`.
    ///
    /// Mutates Coord-side `global_world_manager` state immediately and
    /// DEFERS all Send-side state-machine work to the next
    /// [`SendHandle::apply_pending_send_preamble`] (via a
    /// `ScopeChange::ConfigureReplication` payload) and all World-side
    /// component-hook registration to
    /// [`CoordHandle::apply_pending_world_hooks`] (or its `SendHandle`
    /// twin), which the Sim system calls with the `&mut World`.
    ///
    /// Wire-byte-identical to `InternalWorldServer::configure_entity_replication`
    /// once both drains have run. The existing `InternalWorldServer` method is
    /// unchanged for backward compat.
    pub fn configure_entity_replication(
        &mut self,
        world_entity: &E,
        config: crate::ReplicationConfig,
    ) where
        E: 'static,
    {
        use crate::server::configure_replication::{
            ConfigureCapture, ConfigureSendOp, ConfigureWorldOp,
        };
        use crate::server::scope_change::ScopeChange;
        use naia_shared::Publicity;

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

        // --- Read PRE-transition state (read-before-write capture) ---
        let entity_owner = self
            .shared
            .global_world_manager
            .read()
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned: bool = entity_owner.is_server();
        let client_owned: bool = entity_owner.is_client();
        // Mirror the legacy `client_origin` derivation (world_server.rs:1448).
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
            // Fully identical — no-op (matches legacy early return).
            return;
        }

        let mut send_ops: Vec<ConfigureSendOp<E>> = Vec::new();
        let mut world_ops: Vec<ConfigureWorldOp<E>> = Vec::new();

        // Handle publicity state machine only when publicity changed —
        // this branch structure mirrors `world_server.rs:1464` verbatim.
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
                            self.capture_publish(
                                &global_entity,
                                world_entity,
                                true,
                                &mut send_ops,
                                &mut world_ops,
                            );
                        }
                        Publicity::Delegated => {
                            // private -> delegated
                            self.capture_publish(
                                &global_entity,
                                world_entity,
                                true,
                                &mut send_ops,
                                &mut world_ops,
                            );
                            self.capture_enable_delegation(
                                &global_entity,
                                world_entity,
                                client_origin,
                                &mut send_ops,
                                &mut world_ops,
                            );
                        }
                    }
                }
                Publicity::Public => match config.publicity {
                    Publicity::Private => {
                        // public -> private
                        if server_owned {
                            panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                        }
                        self.capture_unpublish(
                            &global_entity,
                            world_entity,
                            true,
                            &mut send_ops,
                            &mut world_ops,
                        );
                    }
                    Publicity::Public => {
                        unreachable!("publicity prev == next but outer check passed");
                    }
                    Publicity::Delegated => {
                        // public -> delegated
                        self.capture_enable_delegation(
                            &global_entity,
                            world_entity,
                            client_origin,
                            &mut send_ops,
                            &mut world_ops,
                        );
                    }
                },
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
                            self.capture_disable_delegation(
                                &global_entity,
                                world_entity,
                                &mut send_ops,
                                &mut world_ops,
                            );
                            self.capture_unpublish(
                                &global_entity,
                                world_entity,
                                true,
                                &mut send_ops,
                                &mut world_ops,
                            );
                        }
                        Publicity::Public => {
                            // delegated -> public
                            self.capture_disable_delegation(
                                &global_entity,
                                world_entity,
                                &mut send_ops,
                                &mut world_ops,
                            );
                        }
                        Publicity::Delegated => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                    }
                }
            }
        }

        // Always persist the scope_exit field regardless of whether
        // publicity changed (mirrors world_server.rs:1543). Coord-side
        // write — do it immediately.
        self.shared
            .global_world_manager
            .write()
            .entity_set_scope_exit(&global_entity, config.scope_exit);

        // Queue the deferred Send-side work (if any).
        if !send_ops.is_empty() {
            self.shared
                .scope_change_queue
                .lock()
                .push_back(ScopeChange::ConfigureReplication(ConfigureCapture {
                    send_ops,
                }));
        }
        // Queue the deferred World-side hooks (if any).
        if !world_ops.is_empty() {
            let mut q = self.shared.pending_world_hooks.lock();
            for op in world_ops {
                q.push_back(op);
            }
        }
    }

    /// Coord-immediate `publish_entity` body split: do the gwm
    /// `entity_publish` write now (so same-tick reads compose) and
    /// record the Send-side + World-side leaf work. Mirrors
    /// `InternalWorldServer::publish_entity` (`world_server.rs:2531`).
    fn capture_publish(
        &mut self,
        global_entity: &naia_shared::GlobalEntity,
        world_entity: &E,
        server_origin: bool,
        send_ops: &mut Vec<crate::server::configure_replication::ConfigureSendOp<E>>,
        world_ops: &mut Vec<crate::server::configure_replication::ConfigureWorldOp<E>>,
    ) {
        use crate::server::configure_replication::{ConfigureSendOp, ConfigureWorldOp};

        if server_origin {
            // Resolve the owner address from PRE-transition gwm + the
            // sim_handle user store, exactly as `publish_entity` does before
            // `gwm.entity_publish`. The owner MUST be a Client here.
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
            if let Some(addr) = self.state.user_store.address(&user_key) {
                send_ops.push(ConfigureSendOp::Publish {
                    global_entity: *global_entity,
                    owner_addr: addr,
                });
            }
        }

        // Coord-side gwm write — immediate.
        let result = self
            .shared
            .global_world_manager
            .write()
            .entity_publish(global_entity);
        if result {
            // World hook + scope re-eval are deferred (legacy runs them
            // only when `result == true`).
            world_ops.push(ConfigureWorldOp::Publish {
                world_entity: *world_entity,
            });
            send_ops.push(ConfigureSendOp::PublishScopeReeval {
                global_entity: *global_entity,
            });
        }
    }

    /// Coord-immediate `unpublish_entity` body split. The `owner_addr`
    /// capture happens BEFORE `gwm.entity_unpublish` transitions
    /// ClientPublic → Client — this is the B.2 blocker-1 read-before-write
    /// fix. Mirrors `InternalWorldServer::unpublish_entity` (`world_server.rs:2585`).
    fn capture_unpublish(
        &mut self,
        global_entity: &naia_shared::GlobalEntity,
        world_entity: &E,
        _server_origin: bool,
        send_ops: &mut Vec<crate::server::configure_replication::ConfigureSendOp<E>>,
        world_ops: &mut Vec<crate::server::configure_replication::ConfigureWorldOp<E>>,
    ) {
        use crate::server::configure_replication::{ConfigureSendOp, ConfigureWorldOp};

        // Capture the owner address while gwm still holds ClientPublic.
        let owner_addr: Option<std::net::SocketAddr> = self
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
            .and_then(|k| self.state.user_store.address(&k));

        send_ops.push(ConfigureSendOp::Unpublish {
            global_entity: *global_entity,
            owner_addr,
        });

        // Coord-side gwm write — immediate.
        self.shared
            .global_world_manager
            .write()
            .entity_unpublish(global_entity);

        // Deregister each component from the diff handler (Coord-side
        // gwm write) — immediate, mirrors world_server.rs:2621.
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

        world_ops.push(ConfigureWorldOp::Unpublish {
            world_entity: *world_entity,
        });
    }

    /// Coord-immediate `entity_enable_delegation` body split. Mirrors
    /// `InternalWorldServer::entity_enable_delegation` (`world_server.rs:2649`).
    fn capture_enable_delegation(
        &mut self,
        global_entity: &naia_shared::GlobalEntity,
        world_entity: &E,
        client_origin: Option<UserKey>,
        send_ops: &mut Vec<crate::server::configure_replication::ConfigureSendOp<E>>,
        world_ops: &mut Vec<crate::server::configure_replication::ConfigureWorldOp<E>>,
    ) {
        use crate::server::configure_replication::{ConfigureSendOp, ConfigureWorldOp};

        // Per-connection `send_enable_delegation` fan-out — deferred.
        send_ops.push(ConfigureSendOp::EnableDelegationFanout {
            global_entity: *global_entity,
            client_origin,
        });

        if let Some(client_key) = client_origin {
            // The client-owned delegation migration sequence — protocol
            // ordered, deferred. The gwm `entity_publish` (race-path) +
            // `migrate_entity_to_server` + `entity_enable_delegation` +
            // `client_request_authority` writes happen inside the Send
            // drain because they are interleaved with per-connection
            // migration state that only Send owns. The WORLD hooks for
            // this path are ALSO pushed by the Send drain (not here):
            // the legacy `enable_delegation_client_owned_entity` calls
            // `world.entity_publish` (only on the Client→ClientPublic
            // race-path) BEFORE `world.entity_enable_delegation`, and
            // whether the race-path fires is only known at drain time
            // (it reads the gwm owner then). Pushing the EnableDelegation
            // world op here would (a) wrongly order it before a possible
            // Publish op and (b) emit it even if the race-path aborts.
            // So `apply_delegation_migrate` enqueues the world ops in the
            // correct order during the drain.
            send_ops.push(ConfigureSendOp::DelegationMigrate {
                global_entity: *global_entity,
                world_entity: *world_entity,
                client_key,
            });
            // NOTE: no `world_ops.push` here — see above.
        } else {
            // Server-origin delegation — Coord-side gwm write immediate.
            self.shared
                .global_world_manager
                .write()
                .entity_enable_delegation(global_entity);
            world_ops.push(ConfigureWorldOp::EnableDelegation {
                world_entity: *world_entity,
            });
        }
    }

    /// Coord-immediate `entity_disable_delegation` body split. Mirrors
    /// `InternalWorldServer::entity_disable_delegation` (`world_server.rs:2914`).
    fn capture_disable_delegation(
        &mut self,
        global_entity: &naia_shared::GlobalEntity,
        world_entity: &E,
        send_ops: &mut Vec<crate::server::configure_replication::ConfigureSendOp<E>>,
        world_ops: &mut Vec<crate::server::configure_replication::ConfigureWorldOp<E>>,
    ) {
        use crate::server::configure_replication::{ConfigureSendOp, ConfigureWorldOp};

        send_ops.push(ConfigureSendOp::DisableDelegationFanout {
            global_entity: *global_entity,
        });

        // Coord-side gwm write — immediate.
        self.shared
            .global_world_manager
            .write()
            .entity_disable_delegation(global_entity);

        world_ops.push(ConfigureWorldOp::DisableDelegation {
            world_entity: *world_entity,
        });
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — drain the
    /// pending World-side `configure_entity_replication` hook ops onto
    /// `world`, installing per-component diff mutators against the
    /// (already-transitioned) gwm.
    ///
    /// Called by the Sim system each tick (it holds the `&mut World`).
    /// Idempotent: re-call drains an empty queue. Pairs with the Send
    /// drain inside `apply_pending_send_preamble`. See
    /// [`crate::SendHandle::apply_pending_world_hooks`] for the twin on
    /// the Send handle.
    pub fn apply_pending_world_hooks<W>(&self, world: &mut W)
    where
        W: naia_shared::WorldMutType<E>,
        E: 'static,
    {
        crate::server::send_state::drain_pending_world_hooks(&self.shared, world);
    }

    // ====================================================================
    // MISSION_PIPELINE_API_BOUNDARY task #9 — coord-resident reads backing
    // the Room/User borrow-builders on the Pipelined arm. Each mirrors the
    // namesake `InternalWorldServer` helper field-for-field (all read
    // `state.room_store` / `state.user_store` — coord-resident).
    // ====================================================================

    /// Whether `user_key` is a member of `room_key`. Mirrors
    /// `InternalWorldServer::room_has_user`.
    pub fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        self.state.room_store.has_user(room_key, user_key)
    }

    /// Number of users in `room_key`. Mirrors `InternalWorldServer::room_users_count`.
    pub fn room_users_count(&self, room_key: &RoomKey) -> usize {
        self.state.room_store.users_count(room_key)
    }

    /// Iterator over the user keys in `room_key`. Borrows `state.room_store`.
    /// Mirrors `InternalWorldServer::room_user_keys`.
    pub fn room_user_keys(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        self.state.room_store.user_keys_iter(room_key)
    }

    /// Whether `world_entity` is in `room_key` (converts via the global map).
    /// Mirrors `InternalWorldServer::room(..).has_entity`.
    pub fn room_has_entity(&self, room_key: &RoomKey, world_entity: &E) -> bool {
        if let Ok(global_entity) = self.entity_to_global_entity(world_entity) {
            self.state.room_store.has_entity(room_key, &global_entity)
        } else {
            false
        }
    }

    /// All world-entities in `room_key` (converts each via the global map,
    /// dropping any that no longer resolve). Mirrors
    /// `InternalWorldServer::room(..).entities`.
    pub fn room_entities(&self, room_key: &RoomKey) -> Vec<E> {
        let mut output = Vec::new();
        for global_entity in self.state.room_store.entities_iter(room_key) {
            if let Ok(entity) = self.global_entity_to_entity(global_entity) {
                output.push(entity);
            }
        }
        output
    }

    /// Number of entities in `room_key`. Mirrors `InternalWorldServer::room_entities_count`.
    pub fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        self.state.room_store.entities_count(room_key)
    }

    /// Number of rooms `user_key` belongs to, or `None` if the key is stale.
    /// Mirrors `InternalWorldServer::user_rooms_count`.
    pub fn user_rooms_count(&self, user_key: &UserKey) -> Option<usize> {
        self.state.user_store.rooms_count(user_key)
    }

    /// Iterator over the room keys `user_key` belongs to, or `None` if stale.
    /// Borrows `state.user_store`. Mirrors `InternalWorldServer::user_room_keys`.
    pub fn user_room_keys(&self, user_key: &UserKey) -> Option<Iter<'_, RoomKey>> {
        self.state.user_store.room_keys_iter(user_key)
    }

    /// Read-only handle to the **sender-wide (global)** priority state for
    /// `entity` — coord-resident (`state.global_priority_mirror`, 4-E.2e).
    /// Mirrors `InternalWorldServer::global_entity_priority`.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        self.state.global_priority_mirror.get_ref(entity)
    }

    /// Mutable handle to the **sender-wide (global)** priority state for
    /// `entity`. Writes target `state.global_priority_mirror`; the publish step
    /// at the top of the next send carries the change to `send.global_priority`
    /// (4-E.2e). Mirrors `InternalWorldServer::global_entity_priority_mut`.
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        self.state.global_priority_mirror.get_mut(entity)
    }

    /// Read-only handle to the **per-user** priority state for `entity` on
    /// `user_key` (task #13). Reads the per-tick coord staging (the pending
    /// writes for THIS tick) — the live accumulator lives send-side and is not
    /// reachable from coord, so cross-tick accumulator reads return defaults.
    /// Wire-irrelevant (reads never affect output). Mirrors
    /// `InternalWorldServer::user_entity_priority`.
    pub fn user_entity_priority(&self, user_key: &UserKey, entity: E) -> EntityPriorityRef<'_, E> {
        match self.state.user_priority_staging.get(user_key) {
            Some(layer) => layer.get_ref(entity),
            None => EntityPriorityRef::empty(entity),
        }
    }

    /// Mutable handle to the **per-user** priority state for `entity` on
    /// `user_key` (task #13). Writes target the per-tick coord staging
    /// (`state.user_priority_staging`); the next `send`'s preamble drains it into
    /// `SendState.user_priorities` via `drain_merge_into` (gain-dirty aware,
    /// accumulator-preserving). Byte-identical to the resident direct write.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        self.state
            .user_priority_staging
            .entry(*user_key)
            .or_default()
            .get_mut(entity)
    }
}
