//! Step definition modules organized by contract area.
//!
//! Each submodule corresponds to a contract specification and contains
//! step bindings for scenarios testing that contract's obligations.

pub mod client_events;
pub mod common;
pub mod connection;
pub mod entity_authority;
pub mod entity_delegation;
pub mod entity_ownership;
pub mod entity_publication;
pub mod entity_replication;
pub mod entity_scopes;
pub mod messaging;
pub mod observability;
pub mod scope_exit;
pub mod scope_propagation;
pub mod server_events;
pub mod update_candidate_set;
pub mod smoke;
pub mod transport;
pub mod world_integration;

// ABI proof tests (compile_fail demonstrations - permanently disabled)
#[cfg(any())]
mod _abi_proofs;
