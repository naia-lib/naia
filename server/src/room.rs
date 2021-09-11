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
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for entity_key in self.entities.iter() {
            self.entity_removal_queue
                .push_back((*user_key, *entity_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn add_entity(&mut self, entity_key: &EntityKey) {
        self.entities.insert(*entity_key);
    }

    pub fn remove_entity(&mut self, entity_key: &EntityKey) {
        self.entities.remove(entity_key);
        for user_key in self.users.iter() {
            self.entity_removal_queue
                .push_back((*user_key, *entity_key));
        }
    }

    pub fn entities_iter(&self) -> Iter<EntityKey> {
        return self.entities.iter();
    }

    pub fn pop_entity_removal_queue(&mut self) -> Option<(UserKey, EntityKey)> {
        return self.entity_removal_queue.pop_front();
    }
}
