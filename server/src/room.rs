use std::{
    collections::{hash_set::Iter, HashSet, VecDeque},
    hash::Hash,
};

use naia_shared::{BigMapKey, Channel, ChannelKind, GlobalEntity, Message};

use super::{server::InternalWorldServer, user::UserKey};
use crate::PipelinedWorldServer;

/// Opaque handle to a room on the server.
///
/// Obtained from [`Server::create_room`] and used to reference the room in
/// subsequent API calls. `RoomKey` values are stable for the lifetime of the
/// room and may be stored freely.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct RoomKey(u64);

impl BigMapKey for RoomKey {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        RoomKey(value)
    }
}

// Room
pub struct Room {
    users: HashSet<UserKey>,
    entities: HashSet<GlobalEntity>,
    entity_removal_queue: VecDeque<(UserKey, GlobalEntity)>,
}

impl Room {
    pub(crate) fn new() -> Room {
        Self {
            users: HashSet::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    // Users

    pub(crate) fn has_user(&self, user_key: &UserKey) -> bool {
        self.users.contains(user_key)
    }

    pub(crate) fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub(crate) fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for entity in self.entities.iter() {
            self.entity_removal_queue.push_back((*user_key, *entity));
        }
    }

    pub(crate) fn user_keys(&'_ self) -> Iter<'_, UserKey> {
        self.users.iter()
    }

    pub(crate) fn users_count(&self) -> usize {
        self.users.len()
    }

    // Entities

    pub(crate) fn add_entity(&mut self, global_entity: &GlobalEntity) {
        self.entities.insert(*global_entity);
    }

    pub(crate) fn remove_entity(
        &mut self,
        global_entity: &GlobalEntity,
        entity_is_despawned: bool,
    ) -> bool {
        if self.entities.remove(global_entity) {
            if !entity_is_despawned {
                for user_key in self.users.iter() {
                    self.entity_removal_queue
                        .push_back((*user_key, *global_entity));
                }
            }
            true
        } else {
            panic!("Room does not contain Entity");
        }
    }

    pub(crate) fn has_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.entities.contains(global_entity)
    }

    pub(crate) fn entities(&'_ self) -> Iter<'_, GlobalEntity> {
        self.entities.iter()
    }

    pub(crate) fn pop_entity_removal_queue(&mut self) -> Option<(UserKey, GlobalEntity)> {
        self.entity_removal_queue.pop_front()
    }

    /// Number of `(user, entity)` removals queued for the fused Loop 1 drain.
    #[cfg(test)]
    pub(crate) fn entity_removal_queue_len(&self) -> usize {
        self.entity_removal_queue.len()
    }

    /// Drop every queued removal without acting on it. Returns how many were
    /// dropped. Used by the split engine, whose send half resolves room exits
    /// through the scope-change queue instead and never pops this one.
    pub(crate) fn clear_entity_removal_queue(&mut self) -> usize {
        let n = self.entity_removal_queue.len();
        self.entity_removal_queue.clear();
        n
    }

    pub(crate) fn entities_count(&self) -> usize {
        self.entities.len()
    }
}

// room references

/// Which engine shape a [`RoomRef`]/[`RoomMut`] acts on (task #9). Mirrors
/// [`crate::world::entity_mut::EntityMutTarget`]: ONE builder type over both
/// variants. Room state is **coord-resident** (`room_store`), so the Pipelined
/// arm uses [`PipelinedWorldServer`]'s coord fast paths directly (no panic);
/// `broadcast_message` is the lone send-touching op and uses D6 outbound-message
/// staging, exactly like the unified `send_message`.
enum RoomRefTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s InternalWorldServer<E>),
    Pipelined(&'s PipelinedWorldServer<E>),
}

enum RoomMutTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s mut InternalWorldServer<E>),
    Pipelined(&'s mut PipelinedWorldServer<E>),
}

/// Scoped read-only handle for a server room.
///
/// Obtained from [`Server::room`]. Lets you inspect membership (users and
/// entities) without borrowing the server mutably.
pub struct RoomRef<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: RoomRefTarget<'s, E>,
    key: RoomKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> RoomRef<'s, E> {
    pub(crate) fn new(server: &'s InternalWorldServer<E>, key: &RoomKey) -> Self {
        Self {
            server: RoomRefTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s PipelinedWorldServer<E>, key: &RoomKey) -> Self {
        Self {
            server: RoomRefTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns the [`RoomKey`] for this room.
    pub fn key(&self) -> RoomKey {
        self.key
    }

    // Users

    /// Returns `true` if the given user is currently a member of this room.
    pub fn has_user(&self, user_key: &UserKey) -> bool {
        match &self.server {
            RoomRefTarget::Resident(ws) => ws.room_has_user(&self.key, user_key),
            RoomRefTarget::Pipelined(ps) => ps.room_has_user(&self.key, user_key),
        }
    }

    /// Returns the number of users currently in this room.
    pub fn users_count(&self) -> usize {
        match &self.server {
            RoomRefTarget::Resident(ws) => ws.room_users_count(&self.key),
            RoomRefTarget::Pipelined(ps) => ps.room_users_count(&self.key),
        }
    }

    /// Returns an iterator over the [`UserKey`]s of all users in the room.
    pub fn user_keys(&self) -> impl Iterator<Item = &UserKey> {
        let iter: Box<dyn Iterator<Item = &UserKey> + '_> = match &self.server {
            RoomRefTarget::Resident(ws) => Box::new(ws.room_user_keys(&self.key)),
            RoomRefTarget::Pipelined(ps) => Box::new(ps.room_user_keys(&self.key)),
        };
        iter
    }

    // Entities

    /// Returns `true` if the given entity is currently a member of this room.
    pub fn has_entity(&self, entity: &E) -> bool {
        match &self.server {
            RoomRefTarget::Resident(ws) => resident_room_has_entity(ws, &self.key, entity),
            RoomRefTarget::Pipelined(ps) => ps.room_has_entity(&self.key, entity),
        }
    }

    /// Returns the number of entities currently in this room.
    pub fn entities_count(&self) -> usize {
        match &self.server {
            RoomRefTarget::Resident(ws) => ws.room_entities_count(&self.key),
            RoomRefTarget::Pipelined(ps) => ps.room_entities_count(&self.key),
        }
    }

    /// Returns all entity identifiers currently in this room.
    pub fn entities(&self) -> Vec<E> {
        match &self.server {
            RoomRefTarget::Resident(ws) => resident_room_entities(ws, &self.key),
            RoomRefTarget::Pipelined(ps) => ps.room_entities(&self.key),
        }
    }
}

/// Scoped mutable handle for a server room.
///
/// Obtained from [`Server::room_mut`]. Lets you add/remove users and entities,
/// broadcast messages, and destroy the room.
pub struct RoomMut<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: RoomMutTarget<'s, E>,
    key: RoomKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> RoomMut<'s, E> {
    pub(crate) fn new(server: &'s mut InternalWorldServer<E>, key: &RoomKey) -> Self {
        Self {
            server: RoomMutTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s mut PipelinedWorldServer<E>, key: &RoomKey) -> Self {
        Self {
            server: RoomMutTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns the [`RoomKey`] for this room.
    pub fn key(&self) -> RoomKey {
        self.key
    }

    /// Destroys the room. All users are removed from the room; entities that
    /// have no other in-scope path will receive despawn events on those users.
    pub fn destroy(&mut self) {
        match &mut self.server {
            RoomMutTarget::Resident(ws) => {
                ws.room_destroy(&self.key);
            }
            RoomMutTarget::Pipelined(ps) => {
                ps.room_destroy(&self.key);
            }
        }
    }

    // Users

    /// Returns `true` if the given user is currently a member of this room.
    pub fn has_user(&self, user_key: &UserKey) -> bool {
        match &self.server {
            RoomMutTarget::Resident(ws) => ws.room_has_user(&self.key, user_key),
            RoomMutTarget::Pipelined(ps) => ps.room_has_user(&self.key, user_key),
        }
    }

    /// Adds the given user to this room.
    ///
    /// All entities currently in the room that pass the user's scope check
    /// will begin replicating to that user.
    pub fn add_user(&mut self, user_key: &UserKey) -> &mut Self {
        match &mut self.server {
            RoomMutTarget::Resident(ws) => ws.room_add_user(&self.key, user_key),
            RoomMutTarget::Pipelined(ps) => ps.room_add_user(&self.key, user_key),
        }
        self
    }

    /// Removes the given user from this room.
    ///
    /// Entities that are no longer in scope for the user (via any room or
    /// direct scope include) will be despawned on that user's side.
    pub fn remove_user(&mut self, user_key: &UserKey) -> &mut Self {
        match &mut self.server {
            RoomMutTarget::Resident(ws) => ws.room_remove_user(&self.key, user_key),
            RoomMutTarget::Pipelined(ps) => ps.room_remove_user(&self.key, user_key),
        }
        self
    }

    /// Returns the number of users currently in this room.
    pub fn users_count(&self) -> usize {
        match &self.server {
            RoomMutTarget::Resident(ws) => ws.room_users_count(&self.key),
            RoomMutTarget::Pipelined(ps) => ps.room_users_count(&self.key),
        }
    }

    /// Returns an iterator over the [`UserKey`]s of all users in the room.
    pub fn user_keys(&self) -> impl Iterator<Item = &UserKey> {
        let iter: Box<dyn Iterator<Item = &UserKey> + '_> = match &self.server {
            RoomMutTarget::Resident(ws) => Box::new(ws.room_user_keys(&self.key)),
            RoomMutTarget::Pipelined(ps) => Box::new(ps.room_user_keys(&self.key)),
        };
        iter
    }

    // Entities

    /// Returns `true` if the given entity is currently a member of this room.
    pub fn has_entity(&self, entity: &E) -> bool {
        match &self.server {
            RoomMutTarget::Resident(ws) => resident_room_has_entity(ws, &self.key, entity),
            RoomMutTarget::Pipelined(ps) => ps.room_has_entity(&self.key, entity),
        }
    }

    /// Adds the given entity to this room, making it visible to all users in
    /// the room (subject to per-user scope checks).
    pub fn add_entity(&mut self, world_entity: &E) -> &mut Self {
        match &mut self.server {
            RoomMutTarget::Resident(ws) => ws.room_add_entity(&self.key, world_entity),
            RoomMutTarget::Pipelined(ps) => ps.room_add_entity(&self.key, world_entity),
        }
        self
    }

    /// Removes the given entity from this room. Users for whom this was the
    /// only in-scope path will receive a despawn event (unless the entity's
    /// `ScopeExit` is `Persist`).
    pub fn remove_entity(&mut self, world_entity: &E) -> &mut Self {
        match &mut self.server {
            RoomMutTarget::Resident(ws) => ws.room_remove_entity(&self.key, world_entity),
            RoomMutTarget::Pipelined(ps) => ps.room_remove_entity(&self.key, world_entity),
        }
        self
    }

    /// Returns the number of entities currently in this room.
    pub fn entities_count(&self) -> usize {
        match &self.server {
            RoomMutTarget::Resident(ws) => ws.room_entities_count(&self.key),
            RoomMutTarget::Pipelined(ps) => ps.room_entities_count(&self.key),
        }
    }

    // Messages

    /// Broadcasts a message on channel `C` to all users currently in the room.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = message.clone_box();
        let channel_kind = ChannelKind::of::<C>();
        match &mut self.server {
            RoomMutTarget::Resident(ws) => {
                ws.room_broadcast_message(&channel_kind, &self.key, cloned_message)
            }
            RoomMutTarget::Pipelined(ps) => {
                ps.room_broadcast_message(&channel_kind, &self.key, cloned_message)
            }
        }
    }
}

/// Resident-arm helper: `room.has_entity` over the fused `InternalWorldServer`
/// (converts via its in-engine entity converter). The Pipelined arm uses
/// `PipelinedWorldServer::room_has_entity`, which converts via the coord handle.
fn resident_room_has_entity<E: Copy + Eq + Hash + Send + Sync + 'static>(
    server: &InternalWorldServer<E>,
    key: &RoomKey,
    entity: &E,
) -> bool {
    if let Ok(global_entity) = server.entity_converter().entity_to_global_entity(entity) {
        server.room_has_entity(key, &global_entity)
    } else {
        false
    }
}

/// Resident-arm helper for `room.entities` (see [`resident_room_has_entity`]).
fn resident_room_entities<E: Copy + Eq + Hash + Send + Sync + 'static>(
    server: &InternalWorldServer<E>,
    key: &RoomKey,
) -> Vec<E> {
    let mut output = Vec::new();
    for global_entity in server.room_entities(key) {
        if let Ok(entity) = server
            .entity_converter()
            .global_entity_to_entity(global_entity)
        {
            output.push(entity);
        }
    }
    output
}
