use std::{hash::Hash, net::SocketAddr};

use naia_shared::{BigMapKey, Protocolize};

use crate::{RoomKey, Server};

// UserKey
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
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
    pub address: SocketAddr,
}

impl User {
    pub fn new(address: SocketAddr) -> User {
        User { address }
    }
}

// UserRef

pub struct UserRef<'s, P: Protocolize, E: Copy + Eq + Hash> {
    server: &'s Server<P, E>,
    key: UserKey,
}

impl<'s, P: Protocolize, E: Copy + Eq + Hash> UserRef<'s, P, E> {
    pub fn new(server: &'s Server<P, E>, key: &UserKey) -> Self {
        UserRef { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        return self.server.user_address(&self.key).unwrap();
    }
}

// UserMut
pub struct UserMut<'s, P: Protocolize, E: Copy + Eq + Hash> {
    server: &'s mut Server<P, E>,
    key: UserKey,
}

impl<'s, P: Protocolize, E: Copy + Eq + Hash> UserMut<'s, P, E> {
    pub fn new(server: &'s mut Server<P, E>, key: &UserKey) -> Self {
        UserMut { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        return self.server.user_address(&self.key).unwrap();
    }

    pub fn disconnect(&mut self) {
        self.server.disconnect_user(&self.key);
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
}
