//! Step bindings for Naia BDD tests using Namako macros.
//!
//! This crate provides:
//! - `TestWorld` — A newtype wrapper around `Option<Scenario>` for BDD tests
//! - Step bindings registered via `#[given]`, `#[when]`, `#[then]` macros
//!
//! The `naia_namako` adapter depends on this crate for dispatch.
//!
//! # Architecture
//!
//! `TestWorld` follows the newtype pattern and wraps `Option<Scenario>`.
//! All test state lives in `Scenario` - this crate does NOT add fields to the world.
//! Step bindings delegate to `Scenario` APIs for all operations.

mod world;
mod steps;

pub use world::TestWorld;

// Re-export key types from harness for convenience
pub use naia_test_harness::{
    Scenario, ClientKey, ExpectCtx, TrackedClientEvent, TrackedServerEvent,
};
pub use naia_server::RoomKey;
