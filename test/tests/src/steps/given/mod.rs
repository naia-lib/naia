//! Given-step bindings — preconditions and setup.
//!
//! Bindings here use `TestWorldMut` (mutation allowed) and answer:
//! "what state is the system in before the action under test?"
//!
//! Submodule split:
//! - `setup` — protocol/server/client/room initialization
//! - `state_entity` — entity/component/replication preconditions
//! - `state_scope` — scope/room/authority preconditions
//! - `state_publication` — Public/Private replication multi-client
//! - `state_authority` — delegation multi-client
//! - `state_resources` — replicated resources
//! - `state_network` — RTT/jitter/latency
//! - `state_misc` — disconnect/multi-command/queuing
//!
//! The legacy single-file `state.rs` was split into the above modules
//! during Q3 (2026-05-07) — see `_AGENTS/SDD_QUALITY_DEBT_PLAN.md`.

pub mod setup;
pub mod state_authority;
pub mod state_entity;
pub mod state_misc;
pub mod state_network;
pub mod state_publication;
pub mod state_resources;
pub mod state_scope;
