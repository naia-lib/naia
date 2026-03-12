//! Step bindings for Naia BDD tests using Namako macros.
//!
//! This crate provides:
//! - `TestWorld` — A newtype wrapper around `Option<Scenario>` for BDD tests
//! - `TestWorldMut` — Mutable context for Given/When steps (mutation only)
//! - `TestWorldRef` — Read-only context for Then steps (assertions only)
//! - Step bindings registered via `#[given]`, `#[when]`, `#[then]` macros
//!
//! The `naia_npa` adapter depends on this crate for dispatch.
//!
//! # Architecture
//!
//! `TestWorld` follows the newtype pattern and wraps `Option<Scenario>`.
//! All test state lives in `Scenario` - this crate does NOT add fields to the world.
//! Step bindings use context types for capability separation:
//! - Given/When steps receive `TestWorldMut` (can only mutate)
//! - Then steps receive `TestWorldRef` (can only assert)

mod steps;
mod world;

pub use world::{TestWorld, TestWorldMut, TestWorldRef};

// Re-export key types from harness for convenience
pub use naia_server::RoomKey;
pub use naia_test_harness::{
    ClientKey, ExpectCtx, Scenario, TrackedClientEvent, TrackedServerEvent,
};
