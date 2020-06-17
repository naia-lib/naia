
use std::{
    collections::{
        HashSet,
        hash_set::Iter,
    },
};

use super::{
    entities::{
      entity_key::EntityKey,
    },
    user::{
        UserKey,
    }
};

new_key_type! { pub struct RoomKey; }

pub struct Room {
    users: HashSet<UserKey>,
    entities: HashSet<EntityKey>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            entities: HashSet::new(),
        }
    }

    pub fn add_entity(&mut self, entity_key: &EntityKey) {
        self.entities.insert(*entity_key);
    }

    pub fn remove_entity(&mut self, entity_key: &EntityKey) {
        self.entities.remove(entity_key);
    }

    pub fn entities_iter(&self) -> Iter<EntityKey> {
        return self.entities.iter();
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }
}