//! Then-step bindings — assertions.
//!
//! Bindings here use `TestWorldRef` (read-only) and answer:
//! "what observable property must hold after the action?"
//!
//! Submodule split:
//! - `state_assertions` — observable state predicates
//! - `event_assertions` — event-history predicates
//! - `ordering` — subsequence/order assertions across events
//!
//! Phase A.1: stub. A.3 moves existing Then bindings here.

pub mod event_assertions;
pub mod ordering;
pub mod state_assertions_delegation;
pub mod state_assertions_entity;
pub mod state_assertions_network;
pub mod state_assertions_replication;
