//! `CoordHandle<E>` — bundles the coordinator-thread state with a clone
//! of the cross-thread shared init (`Arc<ServerShared<E>>`) so cyberlith's
//! Recv SubApp can install it as a single resource.
//!
//! The `Arc<ServerShared<E>>` clone here is independent of (but identical
//! to) the clones living on `RecvHandle::state.shared` and
//! `SendHandle::state.shared` — all three point at the same underlying
//! [`ServerShared`] allocation.

use std::{hash::Hash, sync::Arc};

use naia_shared::{EntityAndGlobalEntityConverter, Tick};

use crate::server::ServerShared;
use crate::server::coord_state::CoordinatorState;
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
}
