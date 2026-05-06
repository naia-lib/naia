//! Given-step bindings — preconditions and setup.
//!
//! Bindings here use `TestWorldMut` (mutation allowed) and answer:
//! "what state is the system in before the action under test?"
//!
//! Submodule split:
//! - `setup` — protocol/server/client/room initialization
//! - `state` — entity/component/scope/auth state preconditions
//!
//! Phase A.1: stub. A.3 moves existing Given bindings here.

pub mod setup;
pub mod state;
