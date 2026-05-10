use std::hash::Hash;

use naia_shared::{BigMap, EntityAndGlobalEntityConverter, GlobalEntity, GlobalEntityMap};

use crate::{
    room::{Room, RoomKey},
    server::scope_checks_cache::ScopeChecksCache,
    user::UserKey,
    world::entity_room_map::EntityRoomMap,
};

use super::{scope_change::ScopeChange, user_store::UserStore};

/// Owns the authoritative `Room` collection and exposes all room-level
/// queries. Mutation methods that affect state outside of `rooms` (users,
/// entity-room map, scope cache) accept those structures as parameters so the
/// borrow checker stays happy at the `WorldServer` call sites.
pub(super) struct RoomStore {
    rooms: BigMap<RoomKey, Room>,
}

impl RoomStore {
    pub(super) fn new() -> Self {
        Self {
            rooms: BigMap::new(),
        }
    }

    // ── BigMap delegation ────────────────────────────────────────────────

    pub(super) fn insert(&mut self, room: Room) -> RoomKey {
        self.rooms.insert(room)
    }

    pub(super) fn contains(&self, key: &RoomKey) -> bool {
        self.rooms.contains_key(key)
    }

    pub(super) fn get(&self, key: &RoomKey) -> Option<&Room> {
        self.rooms.get(key)
    }

    pub(super) fn get_mut(&mut self, key: &RoomKey) -> Option<&mut Room> {
        self.rooms.get_mut(key)
    }


    pub(super) fn len(&self) -> usize {
        self.rooms.len()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (RoomKey, &Room)> {
        self.rooms.iter()
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (RoomKey, &mut Room)> {
        self.rooms.iter_mut()
    }

    pub(super) fn keys(&self) -> Vec<RoomKey> {
        self.rooms.iter().map(|(k, _)| k).collect()
    }

    // ── Room-level queries ───────────────────────────────────────────────

    pub(super) fn has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        self.rooms
            .get(room_key)
            .map(|r| r.has_user(user_key))
            .unwrap_or(false)
    }

    pub(super) fn users_count(&self, room_key: &RoomKey) -> usize {
        self.rooms
            .get(room_key)
            .map(|r| r.users_count())
            .unwrap_or(0)
    }

    pub(super) fn user_keys_iter(
        &self,
        room_key: &RoomKey,
    ) -> impl Iterator<Item = &UserKey> {
        self.rooms
            .get(room_key)
            .map(|r| r.user_keys())
            .into_iter()
            .flatten()
    }

    pub(super) fn entities_iter(
        &self,
        room_key: &RoomKey,
    ) -> impl Iterator<Item = &GlobalEntity> {
        self.rooms
            .get(room_key)
            .map(|r| r.entities())
            .into_iter()
            .flatten()
    }

    pub(super) fn has_entity(&self, room_key: &RoomKey, entity: &GlobalEntity) -> bool {
        self.rooms
            .get(room_key)
            .map(|r| r.has_entity(entity))
            .unwrap_or(false)
    }

    pub(super) fn entities_count(&self, room_key: &RoomKey) -> usize {
        self.rooms
            .get(room_key)
            .map(|r| r.entities_count())
            .unwrap_or(0)
    }

    // ── Mutation methods (accept external dependencies as params) ────────

    /// Subscribe `user_key` to `room_key`. Updates the user's room cache,
    /// mirrors new (room, user, entity) tuples into the scope-checks cache,
    /// and returns the `ScopeChange` to enqueue on `WorldServer`.
    pub(super) fn add_user<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        user_store: &mut UserStore,
        entity_map: &GlobalEntityMap<E>,
        cache: &mut ScopeChecksCache<E>,
    ) -> ScopeChange {
        let mut subscribed = false;
        if let Some(user) = user_store.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.subscribe_user(user_key);
                user.cache_room(room_key);
                subscribed = true;
            }
        }
        if subscribed {
            let entities: Vec<E> = self
                .rooms
                .get(room_key)
                .map(|room| {
                    room.entities()
                        .filter_map(|ge| entity_map.global_entity_to_entity(ge).ok())
                        .collect()
                })
                .unwrap_or_default();
            cache.on_user_added_to_room(*room_key, *user_key, entities);
        }
        ScopeChange::UserEnteredRoom(*user_key, *room_key)
    }

    /// Unsubscribe `user_key` from `room_key`. Updates the user's room cache,
    /// evicts (room, user, *) tuples from the scope-checks cache, and returns
    /// the `ScopeChange` to enqueue on `WorldServer`.
    pub(super) fn remove_user<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        user_store: &mut UserStore,
        cache: &mut ScopeChecksCache<E>,
    ) -> ScopeChange {
        if let Some(user) = user_store.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.unsubscribe_user(user_key);
                user.uncache_room(room_key);
                cache.on_user_removed_from_room(*room_key, *user_key);
            }
        }
        ScopeChange::UserLeftRoom(*user_key, *room_key)
    }

    /// Destroy a room: remove all its entities from the entity-room map, then
    /// uncache the room from every member user, evict the room from the
    /// scope-checks cache, and drop the `Room` itself.
    /// Returns `true` if the room existed.
    pub(super) fn destroy<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_store: &mut UserStore,
        entity_room_map: &mut EntityRoomMap,
        cache: &mut ScopeChecksCache<E>,
    ) -> bool {
        self.remove_all_entities(room_key, entity_room_map);

        if self.rooms.contains_key(room_key) {
            let room = self.rooms.remove(room_key).unwrap();
            for user_key in room.user_keys() {
                if let Some(user) = user_store.get_mut(user_key) {
                    user.uncache_room(room_key);
                }
            }
            cache.on_room_destroyed(*room_key);
            true
        } else {
            false
        }
    }

    /// Add `world_entity` to `room_key`. Updates the entity-room map and
    /// scope-checks cache, and returns the `ScopeChange` to enqueue.
    pub(super) fn add_entity<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        world_entity: &E,
        entity_map: &GlobalEntityMap<E>,
        entity_room_map: &mut EntityRoomMap,
        cache: &mut ScopeChecksCache<E>,
    ) -> Option<ScopeChange> {
        let global_entity = entity_map.entity_to_global_entity(world_entity).unwrap();
        let mut added = false;
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.add_entity(&global_entity);
            added = true;
        }
        if !added {
            return None;
        }
        entity_room_map.entity_add_room(&global_entity, room_key);
        let users: Vec<UserKey> = self
            .rooms
            .get(room_key)
            .map(|room| room.user_keys().copied().collect())
            .unwrap_or_default();
        cache.on_entity_added_to_room(*room_key, *world_entity, users);
        Some(ScopeChange::EntityEnteredRoom(global_entity, *room_key))
    }

    /// Remove `world_entity` from `room_key`. Updates the entity-room map and
    /// scope-checks cache.
    pub(super) fn remove_entity<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        world_entity: &E,
        entity_map: &GlobalEntityMap<E>,
        entity_room_map: &mut EntityRoomMap,
        cache: &mut ScopeChecksCache<E>,
    ) {
        let global_entity = entity_map.entity_to_global_entity(world_entity).unwrap();
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.remove_entity(&global_entity, false);
            entity_room_map.remove_from_room(&global_entity, room_key);
            cache.on_entity_removed_from_room(*room_key, *world_entity);
        }
    }

    /// Remove all entities from `room_key`, updating the entity-room map.
    /// Cache cleanup is left to the caller (done via `on_room_destroyed`).
    pub(super) fn remove_all_entities(
        &mut self,
        room_key: &RoomKey,
        entity_room_map: &mut EntityRoomMap,
    ) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            let global_entities: Vec<GlobalEntity> = room.entities().copied().collect();
            for global_entity in global_entities {
                room.remove_entity(&global_entity, false);
                entity_room_map.remove_from_room(&global_entity, room_key);
            }
        }
    }
}
