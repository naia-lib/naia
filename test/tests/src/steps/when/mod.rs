//! When-step bindings — the action under test.
//!
//! Bindings here use `TestWorldMut` (mutation allowed) and answer:
//! "what just happened that we want to observe consequences of?"
//!
//! Submodule split:
//! - `server_actions` — server-initiated state changes
//! - `client_actions` — client-initiated state changes
//! - `network_events` — connection/disconnection/tick-passage events
//!
//! Phase A.1: stub. A.3 moves existing When bindings here.

pub mod client_actions;
pub mod network_events;
pub mod server_actions;
