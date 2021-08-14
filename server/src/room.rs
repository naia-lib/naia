use std::collections::{hash_set::Iter, HashSet, VecDeque};

use super::{replicate::object_key::object_key::ObjectKey, user::user_key::UserKey};
use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    replicates: HashSet<ObjectKey>,
    replicate_removal_queue: VecDeque<(UserKey, ObjectKey)>,
    entities: HashSet<EntityKey>,
    entity_removal_queue: VecDeque<(UserKey, EntityKey)>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            replicates: HashSet::new(),
            replicate_removal_queue: VecDeque::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub fn add_replicate(&mut self, object_key: &ObjectKey) {
        self.replicates.insert(*object_key);
    }

    pub fn remove_replicate(&mut self, object_key: &ObjectKey) {
        self.replicates.remove(object_key);
        for user_key in self.users.iter() {
            self.replicate_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn replicates_iter(&self) -> Iter<ObjectKey> {
        return self.replicates.iter();
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for object_key in self.replicates.iter() {
            self.replicate_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn pop_replicate_removal_queue(&mut self) -> Option<(UserKey, ObjectKey)> {
        return self.replicate_removal_queue.pop_front();
    }

    pub fn add_entity(&mut self, object_key: &EntityKey) {
        self.entities.insert(*object_key);
    }

    pub fn remove_entity(&mut self, object_key: &EntityKey) {
        self.entities.remove(object_key);
        for user_key in self.users.iter() {
            self.entity_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn entities_iter(&self) -> Iter<EntityKey> {
        return self.entities.iter();
    }

    pub fn pop_entity_removal_queue(&mut self) -> Option<(UserKey, EntityKey)> {
        return self.entity_removal_queue.pop_front();
    }
}
