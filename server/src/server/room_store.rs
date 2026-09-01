use std::hash::Hash;

use naia_shared::{BigMap, EntityAndGlobalEntityConverter, GlobalEntity, GlobalEntityMap};

use crate::{
    room::{Room, RoomKey},
    user::UserKey,
};

use super::{
    scope_change::{RoomChange, ScopeChange},
    user_store::UserStore,
};

/// Owns the authoritative `Room` collection and exposes all room-level
/// queries. Mutation methods touch only `rooms` and `user_store`; they return
/// `RoomChange<E>` payloads so the caller can push them onto `scope_change_queue`
/// for the SendState drainer to apply.
pub(crate) struct RoomStore {
    rooms: BigMap<RoomKey, Room>,
}

impl RoomStore {
    pub(super) fn new() -> Self {
        Self {
            rooms: BigMap::new(),
        }
    }

    // ── BigMap delegation ────────────────────────────────────────────────

    pub(crate) fn insert(&mut self, room: Room) -> RoomKey {
        self.rooms.insert(room)
    }

    pub(crate) fn contains(&self, key: &RoomKey) -> bool {
        self.rooms.contains_key(key)
    }

    pub(super) fn get(&self, key: &RoomKey) -> Option<&Room> {
        self.rooms.get(key)
    }

    pub(super) fn get_mut(&mut self, key: &RoomKey) -> Option<&mut Room> {
        self.rooms.get_mut(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.rooms.len()
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (RoomKey, &mut Room)> {
        self.rooms.iter_mut()
    }

    pub(crate) fn keys(&self) -> Vec<RoomKey> {
        self.rooms.iter().map(|(k, _)| k).collect()
    }

    // ── Room-level queries ───────────────────────────────────────────────

    pub(crate) fn has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        self.rooms
            .get(room_key)
            .map(|r| r.has_user(user_key))
            .unwrap_or(false)
    }

    pub(crate) fn users_count(&self, room_key: &RoomKey) -> usize {
        self.rooms
            .get(room_key)
            .map(|r| r.users_count())
            .unwrap_or(0)
    }

    pub(crate) fn user_keys_iter(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        self.rooms
            .get(room_key)
            .map(|r| r.user_keys())
            .into_iter()
            .flatten()
    }

    pub(crate) fn entities_iter(&self, room_key: &RoomKey) -> impl Iterator<Item = &GlobalEntity> {
        self.rooms
            .get(room_key)
            .map(|r| r.entities())
            .into_iter()
            .flatten()
    }

    pub(crate) fn has_entity(&self, room_key: &RoomKey, entity: &GlobalEntity) -> bool {
        self.rooms
            .get(room_key)
            .map(|r| r.has_entity(entity))
            .unwrap_or(false)
    }

    /// Queued Loop 1 removals for `room_key` (0 if the room does not exist).
    #[cfg(test)]
    pub(crate) fn entity_removal_queue_len(&self, room_key: &RoomKey) -> usize {
        self.rooms
            .get(room_key)
            .map(|r| r.entity_removal_queue_len())
            .unwrap_or(0)
    }

    /// Drop every room's queued Loop 1 removals. Returns the total dropped.
    ///
    /// Only the fused `InternalWorldServer::update_entity_scopes` pops these
    /// queues; the split engine resolves the same exits from the
    /// `UserLeftRoom` / `EntityEnteredRoom` scope changes pushed alongside, so
    /// on that engine the queues would otherwise grow with every room churn.
    pub(crate) fn discard_entity_removal_queues(&mut self) -> usize {
        self.rooms
            .iter_mut()
            .map(|(_, r)| r.clear_entity_removal_queue())
            .sum()
    }

    pub(crate) fn remove_global_entity_from_all_rooms(&mut self, entity: &GlobalEntity) {
        for (_, room) in self.rooms.iter_mut() {
            if room.has_entity(entity) {
                room.remove_entity(entity, true);
            }
        }
    }

    pub(crate) fn entities_count(&self, room_key: &RoomKey) -> usize {
        self.rooms
            .get(room_key)
            .map(|r| r.entities_count())
            .unwrap_or(0)
    }

    // ── Mutation methods (Coord-only) ────────────────────────────────────

    /// Subscribe `user_key` to `room_key`. Mutates rooms + user_store only.
    /// Returns (legacy ScopeChange, RoomChange) for the caller to push onto
    /// scope_change_queue. The drainer applies the RoomChange to SendState.
    pub(crate) fn add_user<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        user_store: &mut UserStore,
        entity_map: &GlobalEntityMap<E>,
    ) -> (ScopeChange<E>, RoomChange<E>) {
        let mut subscribed = false;
        if let Some(user) = user_store.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.subscribe_user(user_key);
                user.cache_room(room_key);
                subscribed = true;
            }
        }
        let entities_in_room: Vec<E> = if subscribed {
            self.rooms
                .get(room_key)
                .map(|room| {
                    room.entities()
                        .filter_map(|ge| entity_map.global_entity_to_entity(ge).ok())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        (
            ScopeChange::UserEnteredRoom(*user_key, *room_key),
            RoomChange::UserAdded {
                room_key: *room_key,
                user_key: *user_key,
                entities_in_room,
            },
        )
    }

    /// Unsubscribe `user_key` from `room_key`. Mutates rooms + user_store only.
    /// Returns (legacy ScopeChange, RoomChange) for the caller to push onto
    /// scope_change_queue.
    pub(crate) fn remove_user<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_key: &UserKey,
        user_store: &mut UserStore,
    ) -> (ScopeChange<E>, RoomChange<E>) {
        if let Some(user) = user_store.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.unsubscribe_user(user_key);
                user.uncache_room(room_key);
            }
        }
        (
            ScopeChange::UserLeftRoom(*user_key, *room_key),
            RoomChange::UserRemoved {
                room_key: *room_key,
                user_key: *user_key,
            },
        )
    }

    /// Destroy a room: collect entity snapshot, uncache the room from every
    /// member user, and drop the Room itself.
    /// Returns (existed, Option<RoomChange>) for the caller to push.
    pub(crate) fn destroy<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        user_store: &mut UserStore,
        entity_map: &GlobalEntityMap<E>,
    ) -> (bool, Option<RoomChange<E>>) {
        let removed_entities = self.remove_all_entities(room_key, entity_map);

        if self.rooms.contains_key(room_key) {
            let room = self.rooms.remove(room_key).unwrap();
            for user_key in room.user_keys() {
                if let Some(user) = user_store.get_mut(user_key) {
                    user.uncache_room(room_key);
                }
            }
            (
                true,
                Some(RoomChange::RoomDestroyed {
                    room_key: *room_key,
                    removed_entities,
                }),
            )
        } else {
            (false, None)
        }
    }

    /// Add `world_entity` to `room_key`. Mutates rooms only.
    /// Returns None if room missing; otherwise (legacy ScopeChange, RoomChange).
    pub(crate) fn add_entity<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        world_entity: &E,
        entity_map: &GlobalEntityMap<E>,
    ) -> Option<(ScopeChange<E>, RoomChange<E>)> {
        let global_entity = entity_map.entity_to_global_entity(world_entity).unwrap();
        let mut added = false;
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.add_entity(&global_entity);
            added = true;
        }
        if !added {
            return None;
        }
        let users_in_room: Vec<UserKey> = self
            .rooms
            .get(room_key)
            .map(|room| room.user_keys().copied().collect())
            .unwrap_or_default();
        Some((
            ScopeChange::EntityEnteredRoom(global_entity, *room_key),
            RoomChange::EntityAdded {
                room_key: *room_key,
                world_entity: *world_entity,
                global_entity,
                users_in_room,
            },
        ))
    }

    /// Remove `world_entity` from `room_key`. Mutates rooms only.
    /// Returns None if room or entity not found; otherwise (legacy, RoomChange).
    pub(crate) fn remove_entity<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        world_entity: &E,
        entity_map: &GlobalEntityMap<E>,
    ) -> Option<(ScopeChange<E>, RoomChange<E>)> {
        let global_entity = entity_map.entity_to_global_entity(world_entity).unwrap();
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.remove_entity(&global_entity, false);
            Some((
                ScopeChange::EntityEnteredRoom(global_entity, *room_key),
                RoomChange::EntityRemoved {
                    room_key: *room_key,
                    world_entity: *world_entity,
                    global_entity,
                },
            ))
        } else {
            None
        }
    }

    /// Remove all entities from `room_key`. Returns a snapshot of (E, GlobalEntity) pairs
    /// for the drainer to update entity_room_map via RoomDestroyed.
    pub(super) fn remove_all_entities<E: Copy + Eq + Hash + Send + Sync>(
        &mut self,
        room_key: &RoomKey,
        entity_map: &GlobalEntityMap<E>,
    ) -> Vec<(E, GlobalEntity)> {
        let Some(room) = self.rooms.get_mut(room_key) else {
            return Vec::new();
        };
        let global_entities: Vec<GlobalEntity> = room.entities().copied().collect();
        let mut pairs = Vec::with_capacity(global_entities.len());
        for global_entity in global_entities {
            room.remove_entity(&global_entity, false);
            if let Ok(world_entity) = entity_map.global_entity_to_entity(&global_entity) {
                pairs.push((world_entity, global_entity));
            }
        }
        pairs
    }
}
