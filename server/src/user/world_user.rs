use std::{collections::HashSet, net::SocketAddr};

use crate::RoomKey;

// User

/// World-layer user record: tracks data address and cached room membership.
#[derive(Clone)]
pub struct WorldUser {
    data_addr: SocketAddr,
    rooms_cache: HashSet<RoomKey>,
}

impl WorldUser {
    /// Creates a new `WorldUser` registered at the given data-channel address.
    pub fn new(address: SocketAddr) -> Self {
        Self {
            data_addr: address,
            rooms_cache: HashSet::new(),
        }
    }

    /// Returns the user's data-channel socket address.
    pub fn address(&self) -> SocketAddr {
        self.data_addr
    }

    // Rooms

    pub(crate) fn cache_room(&mut self, room_key: &RoomKey) {
        self.rooms_cache.insert(*room_key);
    }

    pub(crate) fn uncache_room(&mut self, room_key: &RoomKey) {
        self.rooms_cache.remove(room_key);
    }

    pub(crate) fn room_keys(&self) -> &HashSet<RoomKey> {
        &self.rooms_cache
    }

    pub(crate) fn rooms_count(&self) -> usize {
        self.rooms_cache.len()
    }
}
