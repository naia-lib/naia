use std::{hash::Hash, net::SocketAddr};

use naia_shared::Protocolize;

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
