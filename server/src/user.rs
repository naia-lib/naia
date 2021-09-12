use std::net::SocketAddr;

use naia_shared::Timestamp;

#[allow(missing_docs)]
#[allow(unused_doc_comments)]
pub mod user_key {
    // The Key used to get a reference of a User
    new_key_type! { pub struct UserKey; }
}

#[derive(Clone)]
pub struct User {
    pub address: SocketAddr,
    pub timestamp: Timestamp,
}

impl User {
    pub fn new(address: SocketAddr, timestamp: Timestamp) -> User {
        User { address, timestamp }
    }
}

// user references

use naia_shared::ProtocolType;

use crate::{RoomKey, Server, UserKey};

// UserRef

pub struct UserRef<'s, T: ProtocolType> {
    server: &'s Server<T>,
    key: UserKey,
}

impl<'s, T: ProtocolType> UserRef<'s, T> {
    pub fn new(server: &'s Server<T>, key: &UserKey) -> Self {
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
pub struct UserMut<'s, T: ProtocolType> {
    server: &'s mut Server<T>,
    key: UserKey,
}

impl<'s, T: ProtocolType> UserMut<'s, T> {
    pub fn new(server: &'s mut Server<T>, key: &UserKey) -> Self {
        UserMut { server, key: *key }
    }

    pub fn key(&self) -> UserKey {
        self.key
    }

    pub fn address(&self) -> SocketAddr {
        return self.server.get_user_address(&self.key).unwrap();
    }

    pub fn room_enter(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_user(room_key, &self.key);

        self
    }

    pub fn room_leave(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_user(room_key, &self.key);

        self
    }
}
