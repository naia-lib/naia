use naia_serde::SerdeInternal;

use crate::HostType;

/// Authority lifecycle state for a delegated entity as observed by one endpoint.
#[derive(SerdeInternal, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityAuthStatus {
    /// No authority over this entity has been granted to any host.
    Available,
    /// This host has requested authority but the grant has not arrived yet.
    Requested,
    /// This host has been granted authority over the entity.
    Granted,
    /// This host is releasing authority; the release is in flight.
    Releasing,
    /// This host was denied authority because another host claimed it first.
    Denied,
}

impl EntityAuthStatus {
    /// Returns `true` if no host currently holds authority over this entity.
    pub fn is_available(&self) -> bool {
        matches!(self, EntityAuthStatus::Available)
    }

    /// Returns `true` if this host has requested but not yet been granted authority.
    pub fn is_requested(&self) -> bool {
        matches!(self, EntityAuthStatus::Requested)
    }

    /// Returns `true` if this host currently holds authority over the entity.
    pub fn is_granted(&self) -> bool {
        matches!(self, EntityAuthStatus::Granted)
    }

    /// Returns `true` if this host's authority request was denied.
    pub fn is_denied(&self) -> bool {
        matches!(self, EntityAuthStatus::Denied)
    }

    /// Returns `true` if this host is in the process of releasing authority.
    pub fn is_releasing(&self) -> bool {
        matches!(self, EntityAuthStatus::Releasing)
    }
}

/// Combined view of an entity's authority status from a specific endpoint's perspective.
#[derive(Debug)]
pub struct HostEntityAuthStatus {
    host_type: HostType,
    auth_status: EntityAuthStatus,
}

impl HostEntityAuthStatus {
    /// Creates a `HostEntityAuthStatus` for `host_type` at the given `auth_status`.
    pub fn new(host_type: HostType, auth_status: EntityAuthStatus) -> Self {
        Self {
            host_type,
            auth_status,
        }
    }

    /// Can this host transition into `Requested`?
    ///
    /// Authority delegation is a client-initiated flow: the server owns
    /// every entity by default and grants/denies client requests. The
    /// server itself never *requests* authority — it already has it —
    /// so this method only has meaningful semantics for `HostType::Client`.
    ///
    /// Server-side `HostEntityAuthStatus` instances exist (the server
    /// tracks per-entity auth state in `server_auth_handler.rs`), but
    /// the only callers of `can_request` are in
    /// `client/src/world/global_world_manager.rs::entity_request_authority`.
    /// Reaching a `(HostType::Server, *)` arm here means a server-side
    /// caller mistakenly drove the client request flow against a server
    /// auth status — a contract violation worth surfacing loudly.
    pub fn can_request(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => true,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => false,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, status) => unreachable!(
                "can_request() is a client-side authority-flow predicate; \
                 reached with HostType::Server (status={status:?}). The server \
                 owns entities by default and does not 'request' authority — \
                 only the client request path in \
                 client/src/world/global_world_manager.rs::entity_request_authority \
                 should call this."
            ),
        }
    }

    /// Can this host transition into `Releasing`?
    ///
    /// Symmetric to `can_request`: only the client releases authority
    /// (the server grants/revokes). Server-side instances should never
    /// reach this method. The only caller is
    /// `client/src/world/global_world_manager.rs::entity_release_authority`.
    pub fn can_release(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => true,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, status) => unreachable!(
                "can_release() is a client-side authority-flow predicate; \
                 reached with HostType::Server (status={status:?}). Only the \
                 client release path in \
                 client/src/world/global_world_manager.rs::entity_release_authority \
                 should call this."
            ),
        }
    }

    /// Returns `true` if this host may mutate component properties on the entity.
    pub fn can_mutate(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => true,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => false,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    /// Returns `true` if this host may read component values from the entity's delegated properties.
    pub fn can_read(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => true,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => false,
            (HostType::Client, EntityAuthStatus::Releasing) => true,
            (HostType::Client, EntityAuthStatus::Denied) => true,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    /// Returns `true` if this host may write (serialize) delegated entity properties for the wire.
    pub fn can_write(&self) -> bool {
        match (self.host_type, self.auth_status) {
            (HostType::Client, EntityAuthStatus::Available) => false,
            (HostType::Client, EntityAuthStatus::Requested) => false,
            (HostType::Client, EntityAuthStatus::Granted) => true,
            (HostType::Client, EntityAuthStatus::Releasing) => true,
            (HostType::Client, EntityAuthStatus::Denied) => false,
            (HostType::Server, EntityAuthStatus::Available) => true,
            (HostType::Server, EntityAuthStatus::Requested) => true,
            (HostType::Server, EntityAuthStatus::Granted) => true,
            (HostType::Server, EntityAuthStatus::Releasing) => true,
            (HostType::Server, EntityAuthStatus::Denied) => true,
        }
    }

    /// Returns the underlying `EntityAuthStatus` value.
    pub fn status(&self) -> EntityAuthStatus {
        self.auth_status
    }
}

#[cfg(test)]
mod tests {
    //! T0.1 — pin the post-fix invariants: client-side `can_request`/
    //! `can_release` return their documented values, and server-side
    //! callers panic with a contract-violation message rather than the
    //! prior `todo!()` (which masked a hidden invariant as a stub).
    use super::*;

    #[test]
    fn client_can_request_only_when_available() {
        let s = HostEntityAuthStatus::new(HostType::Client, EntityAuthStatus::Available);
        assert!(s.can_request());
        for status in [
            EntityAuthStatus::Requested,
            EntityAuthStatus::Granted,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            let s = HostEntityAuthStatus::new(HostType::Client, status);
            assert!(!s.can_request(), "client must not re-request from {status:?}");
        }
    }

    #[test]
    fn client_can_release_only_when_holding_or_requesting() {
        for status in [EntityAuthStatus::Requested, EntityAuthStatus::Granted] {
            let s = HostEntityAuthStatus::new(HostType::Client, status);
            assert!(s.can_release(), "client must release from {status:?}");
        }
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            let s = HostEntityAuthStatus::new(HostType::Client, status);
            assert!(!s.can_release(), "client must not release from {status:?}");
        }
    }

    #[test]
    #[should_panic(expected = "client-side authority-flow predicate")]
    fn server_can_request_panics_with_contract_message() {
        let s = HostEntityAuthStatus::new(HostType::Server, EntityAuthStatus::Available);
        let _ = s.can_request();
    }

    #[test]
    #[should_panic(expected = "client-side authority-flow predicate")]
    fn server_can_release_panics_with_contract_message() {
        let s = HostEntityAuthStatus::new(HostType::Server, EntityAuthStatus::Granted);
        let _ = s.can_release();
    }
}
