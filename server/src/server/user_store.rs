use std::{
    collections::{hash_set::Iter, HashMap},
    net::SocketAddr,
};

use crate::{
    room::RoomKey,
    user::{UserKey, WorldUser},
};

/// Owns the two maps that track authenticated users:
/// - `users`: live `WorldUser` records keyed by `UserKey`
/// - `disconnected_users`: pending handshake entries keyed by socket address
///
/// Pure query methods are self-contained. Lifecycle methods that affect other
/// `InternalWorldServer` domains (connections, scope, priorities, rooms) stay on
/// `InternalWorldServer` as thin orchestration.
pub(crate) struct UserStore {
    users: HashMap<UserKey, WorldUser>,
    /// Tracks users that have been pre-registered (via `receive_user`) but
    /// whose connection handshake has not yet completed. Removed when the
    /// handshake finalizes.
    disconnected_users: HashMap<SocketAddr, UserKey>,
}

impl UserStore {
    pub(crate) fn new() -> Self {
        Self {
            users: HashMap::new(),
            disconnected_users: HashMap::new(),
        }
    }

    // ── Core map access ──────────────────────────────────────────────────

    pub(crate) fn get(&self, key: &UserKey) -> Option<&WorldUser> {
        self.users.get(key)
    }

    pub(crate) fn get_mut(&mut self, key: &UserKey) -> Option<&mut WorldUser> {
        self.users.get_mut(key)
    }

    pub(crate) fn contains(&self, key: &UserKey) -> bool {
        self.users.contains_key(key)
    }

    pub(crate) fn insert(&mut self, key: UserKey, user: WorldUser) {
        self.users.insert(key, user);
    }

    /// Remove the `WorldUser` record. Does NOT touch `disconnected_users`
    /// (that entry is removed in `take_disconnected` at handshake time).
    pub(crate) fn remove(&mut self, key: &UserKey) -> Option<WorldUser> {
        self.users.remove(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.users.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&UserKey, &WorldUser)> {
        self.users.iter()
    }

    pub(crate) fn keys_copied(&self) -> Vec<UserKey> {
        self.users.keys().copied().collect()
    }

    // ── Convenience queries ───────────────────────────────────────────────

    pub(crate) fn address(&self, key: &UserKey) -> Option<SocketAddr> {
        self.users.get(key).map(|u| u.address())
    }

    pub(crate) fn room_keys_iter(&self, key: &UserKey) -> Option<Iter<'_, RoomKey>> {
        self.users.get(key).map(|u| u.room_keys().iter())
    }

    pub(crate) fn rooms_count(&self, key: &UserKey) -> Option<usize> {
        self.users.get(key).map(|u| u.rooms_count())
    }

    // ── Disconnected-users tracking ───────────────────────────────────────

    /// Register a pre-authenticated user address (before handshake completes).
    pub(crate) fn register_disconnected(&mut self, addr: SocketAddr, key: UserKey) {
        self.disconnected_users.insert(addr, key);
    }

    /// Remove and return the `UserKey` for a pre-authenticated address,
    /// called when a handshake completes.
    pub(crate) fn take_disconnected(&mut self, addr: &SocketAddr) -> Option<UserKey> {
        self.disconnected_users.remove(addr)
    }
}
