use std::{
    collections::{hash_set::Iter, HashSet},
    hash::Hash,
    net::SocketAddr,
};

use naia_shared::{BigMapKey, WorldMutType};

use crate::{RoomKey, Server};

// UserKey
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct UserKey(u64);

impl BigMapKey for UserKey {
    fn to_u64(&self) -> u64 {
        self.0
    }

    fn from_u64(value: u64) -> Self {
        UserKey(value)
    }
}

// User

#[derive(Clone)]
pub struct User {
    auth_addr: Option<SocketAddr>,
    data_addr: Option<SocketAddr>,
    rooms_cache: HashSet<RoomKey>,
}

impl User {
    pub fn new(auth_addr: SocketAddr) -> User {
        Self {
            auth_addr: Some(auth_addr),
            data_addr: None,
            rooms_cache: HashSet::new(),
        }
    }

    pub fn has_address(&self) -> bool {
        self.data_addr.is_some()
    }

    pub fn address(&self) -> SocketAddr {
        self.data_addr.unwrap()
    }

    pub(crate) fn take_auth_address(&mut self) -> SocketAddr {
        self.auth_addr.take().unwrap()
    }

    pub(crate) fn set_address(&mut self, addr: &SocketAddr) {
        self.data_addr = Some(*addr);
    }

    pub(crate) fn cache_room(&mut self, room_key: &RoomKey) {
        self.rooms_cache.insert(*room_key);
    }

    pub(crate) fn uncache_room(&mut self, room_key: &RoomKey) {
        self.rooms_cache.remove(room_key);
    }

    pub(crate) fn room_keys(&self) -> &HashSet<RoomKey> {
        &self.rooms_cache
    }

    pub(crate) fn room_count(&self) -> usize {
        self.rooms_cache.len()
    }
}

// UserRef

pub struct UserRef<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s Server<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserRef<'s, E> {
    pub fn new(server: &'s Server<E>, key: &UserKey) -> Self {
        UserRef { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        self.server.user_address(&self.key).unwrap()
    }

    pub fn room_count(&self) -> usize {
        self.server.user_rooms_count(&self.key).unwrap()
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub fn room_keys(&self) -> impl Iterator<Item = &RoomKey> {
        self.server.user_room_keys(&self.key).unwrap()
    }
}

// UserMut
pub struct UserMut<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s mut Server<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserMut<'s, E> {
    pub fn new(server: &'s mut Server<E>, key: &UserKey) -> Self {
        UserMut { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        self.server.user_address(&self.key).unwrap()
    }

    pub fn disconnect<W: WorldMutType<E>>(&mut self, mut world: W) {
        self.server.user_disconnect(&self.key, &mut world);
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_user(room_key, &self.key);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_user(room_key, &self.key);

        self
    }

    pub fn room_count(&self) -> usize {
        self.server.user_rooms_count(&self.key).unwrap()
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub fn room_keys(&self) -> Iter<RoomKey> {
        self.server.user_room_keys(&self.key).unwrap()
    }
}
