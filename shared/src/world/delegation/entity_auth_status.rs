use naia_serde::SerdeInternal;

use crate::HostType;

#[derive(SerdeInternal, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityAuthStatus {
    // as far as we know, no authority over entity has been granted
    Available,
    // host has requested authority, but it has not yet been granted
    Requested,
    // host has been granted authority over entity
    Granted,
    // host has released authority, but it has not yet completed
    Releasing,
    // host has been denied authority over entity (another host has claimed it)
    Denied,
}

impl EntityAuthStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, EntityAuthStatus::Available)
    }

    pub fn is_requested(&self) -> bool {
        matches!(self, EntityAuthStatus::Requested)
    }

    pub fn is_granted(&self) -> bool {
        matches!(self, EntityAuthStatus::Granted)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, EntityAuthStatus::Denied)
    }

    pub fn is_releasing(&self) -> bool {
        matches!(self, EntityAuthStatus::Releasing)
    }
}

#[derive(Debug)]
pub struct HostEntityAuthStatus {
    host_type: HostType,
    auth_status: EntityAuthStatus,
}

impl HostEntityAuthStatus {
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
