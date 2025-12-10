use std::net::SocketAddr;

use naia_server::{UserRef as NaiaUserRef, UserMut as NaiaUserMut, RoomKey};

use crate::{TestEntity, harness::{ClientKey, users::Users}};

/// Harness wrapper for UserRef that works with ClientKey instead of UserKey
pub struct UserRef<'a> {
    user: NaiaUserRef<'a, TestEntity>,
    users: Users<'a>,
}

impl<'a> UserRef<'a> {
    pub(crate) fn new(user: NaiaUserRef<'a, TestEntity>, users: Users<'a>) -> Self {
        Self { user, users }
    }

    /// Get the ClientKey for this user
    pub fn key(&self) -> Option<ClientKey> {
        let user_key = self.user.key();
        self.users.user_to_client_key(&user_key)
    }

    /// Get the socket address of this user
    pub fn address(&self) -> SocketAddr {
        self.user.address()
    }

    /// Get the number of rooms this user belongs to
    pub fn room_count(&self) -> usize {
        self.user.room_count()
    }

    /// Get all room keys this user belongs to
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.user.room_keys().copied().collect()
    }
}

/// Harness wrapper for UserMut that works with ClientKey instead of UserKey
pub struct UserMut<'a> {
    user: NaiaUserMut<'a, TestEntity>,
    users: Users<'a>,
}

impl<'a> UserMut<'a> {
    pub(crate) fn new(user: NaiaUserMut<'a, TestEntity>, users: Users<'a>) -> Self {
        Self { user, users }
    }

    /// Get the ClientKey for this user
    pub fn key(&self) -> Option<ClientKey> {
        let user_key = self.user.key();
        self.users.user_to_client_key(&user_key)
    }

    /// Get the socket address of this user
    pub fn address(&self) -> SocketAddr {
        self.user.address()
    }

    /// Disconnect this user
    pub fn disconnect(&mut self) {
        self.user.disconnect();
    }

    /// Enter a room
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.user.enter_room(room_key);
        self
    }

    /// Leave a room
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.user.leave_room(room_key);
        self
    }

    /// Get the number of rooms this user belongs to
    pub fn room_count(&self) -> usize {
        self.user.room_count()
    }

    /// Get all room keys this user belongs to
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.user.room_keys().copied().collect()
    }
}

