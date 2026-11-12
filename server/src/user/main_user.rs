use std::{net::SocketAddr, time::Instant};

use crate::{server::MainServer, UserKey};

// MainUser

/// Transport-layer user record: tracks auth/data socket addresses and pending-auth age.
#[derive(Clone)]
pub struct MainUser {
    auth_addr: Option<SocketAddr>,
    data_addr: Option<SocketAddr>,
    /// Tracks when the user was created so a pending-auth timeout can be enforced.
    pub(crate) created_at: Instant,
}

impl MainUser {
    /// Creates a new `MainUser` pending auth, registered at the given auth-channel address.
    #[must_use]
    pub fn new(auth_addr: SocketAddr) -> Self {
        Self {
            auth_addr: Some(auth_addr),
            data_addr: None,
            created_at: Instant::now(),
        }
    }

    /// Returns `true` if the data-channel address has been assigned (handshake complete).
    /// Returns `true` if the data-channel address has been assigned (handshake complete).
    #[must_use]
    pub fn has_address(&self) -> bool {
        self.data_addr.is_some()
    }

    /// Returns the data-channel socket address.
    ///
    /// # Panics
    /// Panics if the handshake has not yet completed (`has_address()` is
    /// `false`). Caller-ordering invariant on the embedding application.
    #[must_use]
    #[expect(
        clippy::unwrap_used,
        reason = "internal invariant: caller must check has_address() first"
    )]
    pub fn address(&self) -> SocketAddr {
        self.data_addr.unwrap()
    }

    /// Returns the data-channel socket address if assigned, or `None` if still pending.
    #[must_use]
    pub fn address_opt(&self) -> Option<SocketAddr> {
        self.data_addr
    }

    pub(crate) fn set_address(&mut self, addr: &SocketAddr) {
        self.data_addr = Some(*addr);
    }

    pub(crate) fn peek_auth_address(&self) -> Option<SocketAddr> {
        self.auth_addr
    }

    /// Consumes the auth-channel address, leaving `None` behind.
    ///
    /// Returns `None` when it has already been taken -- which is exactly what
    /// happens when the application answers the same auth request twice. Only
    /// one answer can be sent, so the caller reports the mistake rather than
    /// unwrapping into a panic that names nothing.
    pub(crate) fn take_auth_address(&mut self) -> Option<SocketAddr> {
        self.auth_addr.take()
    }
}

// MainUserRef

/// Read-only view of a connected user's transport metadata (address, key).
pub struct MainUserRef<'s> {
    server: &'s MainServer,
    key: UserKey,
}

impl<'s> MainUserRef<'s> {
    pub(crate) fn new(server: &'s MainServer, key: UserKey) -> Self {
        Self { server, key }
    }

    /// Returns the [`UserKey`] identifying this user.
    #[must_use]
    pub fn key(&self) -> UserKey {
        self.key
    }

    /// Returns the user's socket address.
    ///
    /// # Panics
    /// Panics if the user is no longer connected. A `MainUserRef` is only
    /// ever handed out for a currently-connected user, so this is a
    /// naia-side invariant, never reachable from peer input.
    #[must_use]
    #[expect(
        clippy::unwrap_used,
        reason = "internal invariant: a MainUserRef names a currently-connected user"
    )]
    pub fn address(&self) -> SocketAddr {
        self.server.user_address(self.key).unwrap()
    }
}
