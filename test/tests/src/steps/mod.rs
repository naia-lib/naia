//! Step definition modules organized by contract area.
//!
//! Each submodule corresponds to a contract specification and contains
//! step bindings for scenarios testing that contract's obligations.

pub mod smoke;
pub mod connection;
pub mod common;
pub mod transport;
pub mod messaging;
pub mod observability;
pub mod entity_scopes;

// ABI proof tests (compile_fail demonstrations - gated with #[cfg(FALSE)])
#[cfg(FALSE)]
mod _abi_proofs;
