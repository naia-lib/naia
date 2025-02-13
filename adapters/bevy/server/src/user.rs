use std::{net::SocketAddr, collections::hash_set::Iter};

use naia_server::{UserRef as InnerUserRef, UserMut as InnerUserMut, UserKey, RoomKey};

use crate::world_entity::WorldEntity;

//// UserRef ////

pub struct UserRef<'a> {
    inner: InnerUserRef<'a, WorldEntity>,
}

impl<'a> UserRef<'a> {
    pub(crate) fn new(inner: InnerUserRef<'a, WorldEntity>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> UserKey {
        self.inner.key()
    }

    pub fn address(&self) -> SocketAddr {
        self.inner.address()
    }

    pub fn room_count(&self) -> usize {
        self.inner.room_count()
    }

    pub fn room_keys(&self) -> impl Iterator<Item = &RoomKey> {
        self.inner.room_keys()
    }
}

//// UserMut ////

pub struct UserMut<'a> {
    inner: InnerUserMut<'a, WorldEntity>,
}

impl<'a> UserMut<'a> {
    pub(crate) fn new(inner: InnerUserMut<'a, WorldEntity>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> UserKey {
        self.inner.key()
    }

    pub fn address(&self) -> SocketAddr {
        self.inner.address()
    }

    pub fn disconnect(&mut self) {
        self.inner.disconnect();
    }

    // Rooms

    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.inner.enter_room(room_key);

        self
    }

    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.inner.leave_room(room_key);

        self
    }

    pub fn room_count(&self) -> usize {
        self.inner.room_count()
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub fn room_keys(&self) -> Iter<RoomKey> {
        self.inner.room_keys()
    }
}