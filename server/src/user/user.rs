use std::{collections::hash_set::Iter, hash::Hash, net::SocketAddr};

use naia_shared::BigMapKey;

use crate::{server::WorldServer, RoomKey};

/// Opaque handle to a connected user.
///
/// Obtained from connection events (`ConnectEvent`) and used to reference a
/// specific connected client in subsequent API calls. `UserKey` values are
/// stable for the lifetime of the connection and may be stored freely; they
/// are invalidated (and must not be used) after the corresponding
/// `DisconnectEvent` fires.
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

/// Scoped read-only handle for a connected user.
///
/// Obtained from [`Server::user`]. Lets you inspect the user's network
/// address and room membership without borrowing the server mutably.
pub struct UserRef<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserRef<'s, E> {
    pub(crate) fn new(server: &'s WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns the [`UserKey`] for this user.
    pub fn key(&self) -> UserKey {
        self.key
    }

    /// Returns the remote [`SocketAddr`] for this connection.
    pub fn address(&self) -> SocketAddr {
        self.server.user_address(&self.key).unwrap()
    }

    /// Returns the number of rooms this user currently belongs to.
    pub fn rooms_count(&self) -> usize {
        self.server.user_rooms_count(&self.key).unwrap()
    }

    /// Returns an iterator over the [`RoomKey`]s of all rooms the user belongs to.
    pub fn room_keys(&self) -> impl Iterator<Item = &RoomKey> {
        self.server.user_room_keys(&self.key).unwrap()
    }
}

/// Scoped mutable handle for a connected user.
///
/// Obtained from [`Server::user_mut`]. Lets you move the user between rooms,
/// read their network address, and queue a disconnect.
pub struct UserMut<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s mut WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserMut<'s, E> {
    pub(crate) fn new(server: &'s mut WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns the [`UserKey`] for this user.
    pub fn key(&self) -> UserKey {
        self.key
    }

    /// Returns the remote [`SocketAddr`] for this connection.
    pub fn address(&self) -> SocketAddr {
        self.server.user_address(&self.key).unwrap()
    }

    /// Queues a graceful disconnect for this user.
    ///
    /// The disconnect is processed at the next tick; a `DisconnectEvent` will
    /// fire once the connection is torn down.
    pub fn disconnect(&mut self) {
        self.server.user_queue_disconnect(&self.key);
    }

    // Rooms

    /// Adds the user to the given room.
    ///
    /// All entities in the room that pass the user's scope check will begin
    /// replicating to this user.
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_user(room_key, &self.key);

        self
    }

    /// Removes the user from the given room.
    ///
    /// Entities that are no longer in scope (via any room or direct scope
    /// include) will be despawned on this user's side.
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_user(room_key, &self.key);

        self
    }

    /// Returns the number of rooms this user currently belongs to.
    pub fn rooms_count(&self) -> usize {
        self.server.user_rooms_count(&self.key).unwrap()
    }

    /// Returns an iterator over the [`RoomKey`]s of all rooms the user belongs to.
    pub fn room_keys(&'_ self) -> Iter<'_, RoomKey> {
        self.server.user_room_keys(&self.key).unwrap()
    }
}
