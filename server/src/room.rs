use std::collections::{hash_set::Iter, HashSet, VecDeque};

use super::{replicate::keys::ObjectKey, user::user_key::UserKey};
use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    objects: HashSet<ObjectKey>,
    object_removal_queue: VecDeque<(UserKey, ObjectKey)>,
    entities: HashSet<EntityKey>,
    entity_removal_queue: VecDeque<(UserKey, EntityKey)>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            objects: HashSet::new(),
            object_removal_queue: VecDeque::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub fn add_object(&mut self, object_key: &ObjectKey) {
        self.objects.insert(*object_key);
    }

    pub fn remove_object(&mut self, object_key: &ObjectKey) {
        self.objects.remove(object_key);
        for user_key in self.users.iter() {
            self.object_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn objects_iter(&self) -> Iter<ObjectKey> {
        return self.objects.iter();
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for object_key in self.objects.iter() {
            self.object_removal_queue.push_back((*user_key, *object_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn pop_object_removal_queue(&mut self) -> Option<(UserKey, ObjectKey)> {
        return self.object_removal_queue.pop_front();
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
