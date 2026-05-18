//! Packaging module for the cyberlith Sim-Owns-World architecture
//! (`cyberlith/_AGENTS/MISSION_SIM_OWNS_WORLD.md`, Phase A.1).
//!
//! Bundles the three pipeline-handle pieces extracted by
//! [`WorldServer::into_pipeline_handles`] into a single entry point that
//! cyberlith's `GameCell::init` can call without re-implementing the split.
//! The three handles — [`CoordHandle`], [`RecvHandle`], [`SendHandle`] —
//! each become the owned state of one of cyberlith's 3 pipelined SubApps
//! (Recv, Sim, Send) or live alongside one (the coord handle lives on the
//! Recv SubApp per D17).
//!
//! Naming note: this module is `pipeline_actors` (not `pipeline_handles`)
//! to avoid collision with the existing `server::pipeline_handles` module
//! that defines [`RecvHandle`] / [`SendHandle`].
//!
//! No behavior change vs. constructing `WorldServer` directly: the
//! packaging is a thin facade over `WorldServer::new` +
//! `into_pipeline_handles`.

mod handles;
mod orchestration;
mod recv_helpers;
mod router;
mod spawn;

pub use handles::CoordHandle;
pub use orchestration::{apply_recv_to_world, run_with_world_server, split_world_server};
pub use recv_helpers::{RecvLifecycleEvent, drain_lifecycle, drain_tick_buffer};
pub use router::TickMessageRouter;
pub use spawn::spawn_server_handles;

#[cfg(test)]
mod tests;
