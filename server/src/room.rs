use std::collections::{hash_set::Iter, HashSet, VecDeque};

use super::{actors::actor_key::actor_key::ActorKey, user::user_key::UserKey};

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod room_key {
    // The Key used to get a reference of a Room
    new_key_type! { pub struct RoomKey; }
}

pub struct Room {
    users: HashSet<UserKey>,
    actors: HashSet<ActorKey>,
    removal_queue: VecDeque<(UserKey, ActorKey)>,
}

impl Room {
    pub fn new() -> Room {
        Room {
            users: HashSet::new(),
            actors: HashSet::new(),
            removal_queue: VecDeque::new(),
        }
    }

    pub fn add_actor(&mut self, actor_key: &ActorKey) {
        self.actors.insert(*actor_key);
    }

    pub fn remove_actor(&mut self, actor_key: &ActorKey) {
        self.actors.remove(actor_key);
        for user_key in self.users.iter() {
            self.removal_queue.push_back((*user_key, *actor_key));
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
            self.removal_queue.push_back((*user_key, *actor_key));
        }
    }

    pub fn users_iter(&self) -> Iter<UserKey> {
        return self.users.iter();
    }

    pub fn pop_removal_queue(&mut self) -> Option<(UserKey, ActorKey)> {
        return self.removal_queue.pop_front();
    }
}
