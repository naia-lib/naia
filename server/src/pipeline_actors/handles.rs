//! `CoordHandle<E>` — bundles the coordinator-thread state with a clone
//! of the cross-thread shared init (`Arc<ServerShared<E>>`) so cyberlith's
//! Recv SubApp can install it as a single resource.
//!
//! The `Arc<ServerShared<E>>` clone here is independent of (but identical
//! to) the clones living on `RecvHandle::state.shared` and
//! `SendHandle::state.shared` — all three point at the same underlying
//! [`ServerShared`] allocation.

use std::{hash::Hash, sync::Arc};

use naia_shared::{EntityAndGlobalEntityConverter, GlobalEntitySpawner, Tick};

use crate::room::{Room, RoomKey};
use crate::server::coord_state::CoordinatorState;
use crate::server::scope_change::ScopeChange;
use crate::server::ServerShared;
use crate::user::UserKey;
use crate::EntityOwner;

/// Coord-half of a pipeline-mode [`crate::WorldServer`].
///
/// Holds [`CoordinatorState`] (`user_store`, `room_store`, scope tables,
/// historian, etc.) plus a clone of the cross-thread `Arc<ServerShared>`
/// so the coord-side code can read shared init (`server_config`, kind
/// tables, `global_dirty`) without going through the recv or send handle.
pub struct CoordHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Coordinator-thread state. Single-threaded — no internal locking.
    pub state: CoordinatorState<E>,
    /// Cross-thread shared init + atomic cells. Cloneable `Arc` — the
    /// same allocation is also referenced by [`crate::RecvHandle::state.shared`]
    /// and [`crate::SendHandle::state.shared`].
    pub shared: Arc<ServerShared<E>>,
}

impl<E: Copy + Eq + Hash + Send + Sync> CoordHandle<E> {
    // ============================================================
    // Phase B.7 — coord-side read API surface for the bevy adapter
    // pipeline bridge (`apply_receive_output_pipeline`).
    // ============================================================
    //
    // All methods below are pure-coord-state reads — no recv/send
    // mutation. They mirror the namesake methods on `WorldServer`.

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
    /// `WorldServer::entity_owner` (which reads only from `shared`).
    ///
    /// The pre-B.7 bevy adapter passed a world reference here for
    /// symmetry with `WorldServer::entity(world, entity).owner()`, but
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
    /// use `WorldServer::user_keys` via `run_with_world_server`, which
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

    /// MISSION_USER_ONLY_SEES_SIM Phase B.1 (2026-05-19) — return a
    /// cloneable [`crate::pipeline_actors::SimConverter`] view over
    /// this server's `Arc<ServerShared<E>>`.
    ///
    /// Cyberlith installs the returned `SimConverter` as a Bevy
    /// `Resource` on the Sim app so Sim systems can construct
    /// `EntityProperty`-bearing messages or components without
    /// reassembling a `WorldServer<E>`. The backing
    /// `EntityAndGlobalEntityConverter<E>` impl on `ServerShared<E>`
    /// delegates to the same `global_entity_map` `RwLock` that
    /// `WorldServer`'s converter reads, so wire output is byte-identical.
    pub fn sim_converter(&self) -> crate::pipeline_actors::SimConverter<E>
    where
        E: 'static,
    {
        let shared_clone: Arc<crate::server::ServerShared<E>> = Arc::clone(&self.shared);
        let arc: Arc<dyn EntityAndGlobalEntityConverter<E> + Send + Sync> = shared_clone;
        crate::pipeline_actors::SimConverter::from_arc(arc)
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
            self.state.room_store.destroy(room_key, &mut self.state.user_store, &*entity_map)
        };
        if let Some(room_change) = room_change_opt {
            self.shared.scope_change_queue.lock().push_back(ScopeChange::RoomChange(room_change));
        }
        existed
    }

    /// Add a user to a room. Push-only; Send drains on next tick.
    pub fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        let (legacy_change, room_change) = {
            let entity_map = self.shared.global_entity_map.read();
            self.state.room_store.add_user(room_key, user_key, &mut self.state.user_store, &*entity_map)
        };
        let mut q = self.shared.scope_change_queue.lock();
        q.push_back(legacy_change);
        q.push_back(ScopeChange::RoomChange(room_change));
    }

    /// Remove a user from a room. Push-only; Send drains on next tick.
    pub fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        let (legacy_change, room_change) = self.state.room_store.remove_user::<E>(
            room_key, user_key, &mut self.state.user_store);
        let mut q = self.shared.scope_change_queue.lock();
        q.push_back(legacy_change);
        q.push_back(ScopeChange::RoomChange(room_change));
    }

    /// Add an entity to a room. Push-only; Send drains on next tick.
    pub fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.state.room_store.add_entity(room_key, world_entity, &*entity_map)
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
    /// between the coord/cyberlith-Sim room mutations and the Send-side
    /// drain. O(1) lock + len().
    pub fn scope_change_queue_len(&self) -> usize {
        self.shared.scope_change_queue.lock().len()
    }

    /// Remove an entity from a room. Push-only; Send drains on next tick.
    pub fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let pair_opt = {
            let entity_map = self.shared.global_entity_map.read();
            self.state.room_store.remove_entity(room_key, world_entity, &*entity_map)
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
    // `WorldServer::enable_entity_replication` (`world_server.rs:1044`)
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
    // `WorldServer::enable_entity_replication` remains unchanged.

    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.1 (2026-05-19) —
    /// register `entity` as a server-owned replicating entity without
    /// reassembling a `WorldServer`.
    ///
    /// Byte-identical to `WorldServer::enable_entity_replication`:
    /// writes the same three shared-state fields
    /// (`global_entity_map`, `global_world_manager`, `idx_to_world`)
    /// in the same order. No Send-side mutation; no world-hook
    /// registration; no deferred drainer work.
    ///
    /// Replaces the `run_with_world_server(coord, recv, send, |ws|
    /// ws.enable_entity_replication(&entity))` reassembly pattern that
    /// cyberlith's `server_access::drain_sim_registrations` and
    /// `drain_sim_tile_registrations` use today.
    pub fn enable_entity_replication(&mut self, world_entity: &E) {
        // Mirror `WorldServer::spawn_entity_inner` field-for-field —
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
            self.shared.idx_to_world.write()[idx.as_usize()] = Some(*world_entity);
        }
    }
}
