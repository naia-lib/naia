use std::hash::Hash;

use naia_shared::GlobalEntity;

use crate::server::configure_replication::ConfigureCapture;
use crate::{RoomKey, UserKey};

/// Events that drive scope re-evaluation.
/// Each variant encodes exactly the (user, entity) pairs that need attention.
pub(crate) enum ScopeChange<E: Copy + Eq + Hash + Send + Sync> {
    /// User was added to a room — check all entities in that room for this user.
    UserEnteredRoom(UserKey, RoomKey),
    /// User was removed from a room — entities in that room may need despawning.
    UserLeftRoom(UserKey, RoomKey),
    /// Entity was added to a room — check all users in that room for this entity.
    EntityEnteredRoom(GlobalEntity, RoomKey),
    /// Explicit include/exclude via UserScope API.
    ScopeToggled(UserKey, GlobalEntity, bool),
    /// Room-level structural change — SendState projection fields (entity_room_map,
    /// scope_checks_cache) are updated by apply_pending_room_changes drainer.
    RoomChange(RoomChange<E>),
    /// MISSION_USER_ONLY_SEES_SIM Phase D.2.2 (2026-05-19) — deferred
    /// Send-side `configure_entity_replication` work. The Coord-only
    /// `CoordHandle::configure_entity_replication` performs the gwm
    /// writes immediately and captures the Send-side leaf ops here; the
    /// `SendState::apply_pending_configure_replication` drainer (driven
    /// from `apply_pending_send_preamble`) executes them. World-side
    /// hooks ride a separate `pending_world_hooks` queue (different
    /// drain site — needs the `&mut World`).
    ConfigureReplication(ConfigureCapture<E>),
}

/// Payloads for room-structural mutations that need to update SendState projection
/// fields (entity_room_map + scope_checks_cache). The drainer apply_pending_room_changes
/// is the single canonical writer to those structures.
pub(crate) enum RoomChange<E: Copy + Eq + Hash + Send + Sync> {
    /// User added to a room — Send must add (room, user, entity) tuples
    /// to scope_checks_cache for every entity already in the room.
    UserAdded {
        room_key: RoomKey,
        user_key: UserKey,
        entities_in_room: Vec<E>,
    },

    /// User removed from a room — Send drops all (room, user, *) tuples.
    UserRemoved {
        room_key: RoomKey,
        user_key: UserKey,
    },

    /// Entity added to a room — Send must:
    ///   1. entity_room_map.entity_add_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_entity_added_to_room(room_key, world_entity, users_in_room)
    EntityAdded {
        room_key: RoomKey,
        world_entity: E,
        global_entity: GlobalEntity,
        users_in_room: Vec<UserKey>,
    },

    /// Entity removed from a room — Send must:
    ///   1. entity_room_map.remove_from_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_entity_removed_from_room(room_key, world_entity)
    EntityRemoved {
        room_key: RoomKey,
        world_entity: E,
        global_entity: GlobalEntity,
    },

    /// Room destroyed — Send must:
    ///   1. for each (world_entity, global_entity) in removed_entities:
    ///        entity_room_map.remove_from_room(&global_entity, &room_key)
    ///   2. scope_checks_cache.on_room_destroyed(room_key)
    RoomDestroyed {
        room_key: RoomKey,
        removed_entities: Vec<(E, GlobalEntity)>,
    },
}
