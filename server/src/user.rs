use std::{collections::HashSet, hash::Hash, net::SocketAddr};

use naia_shared::{Timestamp, ProtocolType};

use crate::{RoomKey, Server, UserKey};

// UserKey

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod user_key {
    // The Key used to get a reference of a User
    new_key_type! { pub struct UserKey; }
}

// User

#[derive(Clone)]
pub struct User {
    pub address: SocketAddr,
}

impl User {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
        }
    }
}

// UserRecord

#[derive(Clone)]
pub struct UserRecord<E: Copy + Eq + Hash> {
    pub user: User,
    pub timestamp: Timestamp,
    pub owned_entities: HashSet<E>,
}

impl<E: Copy + Eq + Hash> UserRecord<E> {
    pub fn new(address: SocketAddr, timestamp: Timestamp) -> UserRecord<E> {
        UserRecord {
            user: User::new(address),
            timestamp,
            owned_entities: HashSet::new(),
        }
    }
}

// UserRef

pub struct UserRef<'s, P: ProtocolType, E: Copy + Eq + Hash> {
    server: &'s Server<P, E>,
    key: UserKey,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash> UserRef<'s, P, E> {
    pub fn new(server: &'s Server<P, E>, key: &UserKey) -> Self {
        UserRef { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        return self.server.get_user_address(&self.key).unwrap();
    }
}

// UserMut
pub struct UserMut<'s, P: ProtocolType, E: Copy + Eq + Hash> {
    server: &'s mut Server<P, E>,
    key: UserKey,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash> UserMut<'s, P, E> {
    pub fn new(server: &'s mut Server<P, E>, key: &UserKey) -> Self {
        UserMut { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        return self.server.get_user_address(&self.key).unwrap();
    }

    pub fn disconnect(&mut self) {
        self.server.user_force_disconnect(&self.key);
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
