use std::collections::{hash_set::Iter, HashSet, VecDeque};

use super::{state::object_key::object_key::ObjectKey, user::user_key::UserKey};
use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    states: HashSet<ObjectKey>,
    state_removal_queue: VecDeque<(UserKey, ObjectKey)>,
    entities: HashSet<EntityKey>,
    entity_removal_queue: VecDeque<(UserKey, EntityKey)>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            states: HashSet::new(),
            state_removal_queue: VecDeque::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub fn add_state(&mut self, object_key: &ObjectKey) {
        self.states.insert(*object_key);
    }

    pub fn remove_state(&mut self, object_key: &ObjectKey) {
        self.states.remove(object_key);
        for user_key in self.users.iter() {
            self.state_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn states_iter(&self) -> Iter<ObjectKey> {
        return self.states.iter();
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for object_key in self.states.iter() {
            self.state_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn pop_state_removal_queue(&mut self) -> Option<(UserKey, ObjectKey)> {
        return self.state_removal_queue.pop_front();
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
