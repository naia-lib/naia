use std::collections::{hash_set::Iter, HashSet, VecDeque};

use super::{actors::actor_key::actor_key::ActorKey, user::user_key::UserKey};
use naia_shared::EntityKey;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    actors: HashSet<ActorKey>,
    actor_removal_queue: VecDeque<(UserKey, ActorKey)>,
    entities: HashSet<EntityKey>,
    entity_removal_queue: VecDeque<(UserKey, EntityKey)>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            actors: HashSet::new(),
            actor_removal_queue: VecDeque::new(),
            entities: HashSet::new(),
            entity_removal_queue: VecDeque::new(),
        }
    }

    pub fn add_actor(&mut self, actor_key: &ActorKey) {
        self.actors.insert(*actor_key);
    }

    pub fn remove_actor(&mut self, actor_key: &ActorKey) {
        self.actors.remove(actor_key);
        for user_key in self.users.iter() {
            self.actor_removal_queue.push_back((*user_key, *actor_key));
        }
    }

    pub fn actors_iter(&self) -> Iter<ActorKey> {
        return self.actors.iter();
    }

    pub fn subscribe_user(&mut self, user_key: &UserKey) {
        self.users.insert(*user_key);
    }

    pub fn unsubscribe_user(&mut self, user_key: &UserKey) {
        self.users.remove(user_key);
        for actor_key in self.actors.iter() {
            self.actor_removal_queue.push_back((*user_key, *actor_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn pop_actor_removal_queue(&mut self) -> Option<(UserKey, ActorKey)> {
        return self.actor_removal_queue.pop_front();
    }

    pub fn add_entity(&mut self, actor_key: &EntityKey) {
        self.entities.insert(*actor_key);
    }

    pub fn remove_entity(&mut self, actor_key: &EntityKey) {
        self.entities.remove(actor_key);
        for user_key in self.users.iter() {
            self.entity_removal_queue.push_back((*user_key, *actor_key));
        }
    }

    pub fn entities_iter(&self) -> Iter<EntityKey> {
        return self.entities.iter();
    }

    pub fn pop_entity_removal_queue(&mut self) -> Option<(UserKey, EntityKey)> {
        return self.entity_removal_queue.pop_front();
    }
}
