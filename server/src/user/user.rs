use std::{collections::hash_set::Iter, hash::Hash, net::SocketAddr};

use naia_shared::BigMapKey;

use crate::{server::InternalWorldServer, PipelinedWorldServer, RoomKey};

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

/// Which engine shape a [`UserRef`]/[`UserMut`] acts on (task #9). User state
/// is **coord-resident** (`user_store`); disconnect + room moves map to existing
/// [`PipelinedWorldServer`] coord ops, so the Pipelined arm never panics.
enum UserRefTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s InternalWorldServer<E>),
    Pipelined(&'s PipelinedWorldServer<E>),
}

enum UserMutTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s mut InternalWorldServer<E>),
    Pipelined(&'s mut PipelinedWorldServer<E>),
}

/// Scoped read-only handle for a connected user.
///
/// Obtained from [`Server::user`]. Lets you inspect the user's network
/// address and room membership without borrowing the server mutably.
pub struct UserRef<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: UserRefTarget<'s, E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> UserRef<'s, E> {
    pub(crate) fn new(server: &'s InternalWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserRefTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s PipelinedWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserRefTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns the [`UserKey`] for this user.
    pub fn key(&self) -> UserKey {
        self.key
    }

    /// Returns the remote [`SocketAddr`] for this connection.
    pub fn address(&self) -> SocketAddr {
        match &self.server {
            UserRefTarget::Resident(ws) => ws.user_address(&self.key),
            UserRefTarget::Pipelined(ps) => ps.user_address(&self.key),
        }
        .unwrap()
    }

    /// Returns the number of rooms this user currently belongs to.
    pub fn rooms_count(&self) -> usize {
        match &self.server {
            UserRefTarget::Resident(ws) => ws.user_rooms_count(&self.key),
            UserRefTarget::Pipelined(ps) => ps.user_rooms_count(&self.key),
        }
        .unwrap()
    }

    /// Returns an iterator over the [`RoomKey`]s of all rooms the user belongs to.
    pub fn room_keys(&self) -> Iter<'_, RoomKey> {
        match &self.server {
            UserRefTarget::Resident(ws) => ws.user_room_keys(&self.key),
            UserRefTarget::Pipelined(ps) => ps.user_room_keys(&self.key),
        }
        .unwrap()
    }
}

/// Scoped mutable handle for a connected user.
///
/// Obtained from [`Server::user_mut`]. Lets you move the user between rooms,
/// read their network address, and queue a disconnect.
pub struct UserMut<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: UserMutTarget<'s, E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> UserMut<'s, E> {
    pub(crate) fn new(server: &'s mut InternalWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserMutTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s mut PipelinedWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserMutTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns the [`UserKey`] for this user.
    pub fn key(&self) -> UserKey {
        self.key
    }

    /// Returns the remote [`SocketAddr`] for this connection.
    pub fn address(&self) -> SocketAddr {
        match &self.server {
            UserMutTarget::Resident(ws) => ws.user_address(&self.key),
            UserMutTarget::Pipelined(ps) => ps.user_address(&self.key),
        }
        .unwrap()
    }

    /// Queues a graceful disconnect for this user.
    ///
    /// The disconnect is processed at the next tick; a `DisconnectEvent` will
    /// fire once the connection is torn down.
    pub fn disconnect(&mut self) {
        match &mut self.server {
            UserMutTarget::Resident(ws) => {
                ws.user_queue_disconnect(&self.key, naia_shared::DisconnectReason::Kicked)
            }
            // Pipelined: coord queues `(key, Kicked)` into
            // `pending_disconnect_requests`; the recv path drains it at the top of
            // `process_disconnects` (D.3b.3) — the established pipelined disconnect.
            UserMutTarget::Pipelined(ps) => ps.disconnect_user(&self.key),
        }
    }

    // Rooms

    /// Adds the user to the given room.
    ///
    /// All entities in the room that pass the user's scope check will begin
    /// replicating to this user.
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        match &mut self.server {
            UserMutTarget::Resident(ws) => ws.room_add_user(room_key, &self.key),
            UserMutTarget::Pipelined(ps) => ps.room_add_user(room_key, &self.key),
        }
        self
    }

    /// Removes the user from the given room.
    ///
    /// Entities that are no longer in scope (via any room or direct scope
    /// include) will be despawned on this user's side.
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        match &mut self.server {
            UserMutTarget::Resident(ws) => ws.room_remove_user(room_key, &self.key),
            UserMutTarget::Pipelined(ps) => ps.room_remove_user(room_key, &self.key),
        }
        self
    }

    /// Returns the number of rooms this user currently belongs to.
    pub fn rooms_count(&self) -> usize {
        match &self.server {
            UserMutTarget::Resident(ws) => ws.user_rooms_count(&self.key),
            UserMutTarget::Pipelined(ps) => ps.user_rooms_count(&self.key),
        }
        .unwrap()
    }

    /// Returns an iterator over the [`RoomKey`]s of all rooms the user belongs to.
    pub fn room_keys(&'_ self) -> Iter<'_, RoomKey> {
        match &self.server {
            UserMutTarget::Resident(ws) => ws.user_room_keys(&self.key),
            UserMutTarget::Pipelined(ps) => ps.user_room_keys(&self.key),
        }
        .unwrap()
    }
}
