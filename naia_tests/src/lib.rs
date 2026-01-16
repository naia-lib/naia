//! Step bindings for Naia BDD tests using Namako macros.
//!
//! This crate provides:
//! - `SmokeWorld` — The World type for BDD scenarios
//! - Step bindings registered via `#[given]`, `#[when]`, `#[then]` macros
//!
//! The `naia_namako` adapter depends on this crate for dispatch.

mod world;

pub use world::SmokeWorld;

// Re-export key types for convenience
pub use naia_test_harness::{Scenario, ClientKey, ExpectCtx};
pub use naia_server::RoomKey;
