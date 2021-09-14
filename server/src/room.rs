use std::collections::{hash_set::Iter, HashSet, VecDeque};

use naia_shared::EntityKey;

use super::user::user_key::UserKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    entities: HashSet<EntityKey>,
    entity_removal_queue: VecDeque<(UserKey, EntityKey)>,
}

impl Room {
    pub(crate) fn new() -> Room {
        Room {
            users: HashSet::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub(crate) fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub(crate) fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for entity_key in self.entities.iter() {
            self.entity_removal_queue
                .push_back((*user_key, *entity_key));
        }
    }

    pub(crate) fn user_keys(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub(crate) fn users_count(&self) -> usize {
        return self.users.len();
    }

    pub(crate) fn add_entity(&mut self, entity_key: &EntityKey) {
        self.entities.insert(*entity_key);
    }

    pub(crate) fn remove_entity(&mut self, entity_key: &EntityKey) {
        self.entities.remove(entity_key);
        for user_key in self.users.iter() {
            self.entity_removal_queue
                .push_back((*user_key, *entity_key));
        }
    }

    pub(crate) fn entity_keys(&self) -> Iter<EntityKey> {
        return self.entities.iter();
    }

    pub(crate) fn pop_entity_removal_queue(&mut self) -> Option<(UserKey, EntityKey)> {
        return self.entity_removal_queue.pop_front();
    }

    pub(crate) fn entities_count(&self) -> usize {
        return self.entities.len();
    }
}

// room references

use naia_shared::ProtocolType;

use crate::Server;

use room_key::RoomKey;

// RoomRef

pub struct RoomRef<'s, P: ProtocolType> {
    server: &'s Server<P>,
    key: RoomKey,
}

impl<'s, P: ProtocolType> RoomRef<'s, P> {
    pub fn new(server: &'s Server<P>, key: &RoomKey) -> Self {
        RoomRef { server, key: *key }
    }

    pub fn key(&self) -> RoomKey {
        self.key
    }

    pub fn users_count(&self) -> usize {
        return self.server.room_users_count(&self.key);
    }

    pub fn entities_count(&self) -> usize {
        return self.server.room_entities_count(&self.key);
    }
}

// RoomMut
pub struct RoomMut<'s, P: ProtocolType> {
    server: &'s mut Server<P>,
    key: RoomKey,
}

impl<'s, P: ProtocolType> RoomMut<'s, P> {
    pub fn new(server: &'s mut Server<P>, key: &RoomKey) -> Self {
        RoomMut { server, key: *key }
    }

    pub fn key(&self) -> RoomKey {
        self.key
    }

    pub fn users_count(&self) -> usize {
        return self.server.room_users_count(&self.key);
    }

    pub fn entities_count(&self) -> usize {
        return self.server.room_entities_count(&self.key);
    }

    pub fn add_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.server.room_add_user(&self.key, user_key);

        self
    }

    pub fn remove_user(&mut self, user_key: &UserKey) -> &mut Self {
        self.server.room_remove_user(&self.key, user_key);

        self
    }

    pub fn add_entity(&mut self, entity_key: &EntityKey) -> &mut Self {
        self.server.room_add_entity(&self.key, entity_key);

        self
    }

    pub fn remove_entity(&mut self, entity_key: &EntityKey) -> &mut Self {
        self.server.room_remove_entity(&self.key, entity_key);

        self
    }

    pub fn destroy(&mut self) {
        self.server.room_destroy(&self.key);
    }
}
