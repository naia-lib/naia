//! Pipeline actors — the three-handle split for pipelined server operation.
//!
//! The three handles — [`CoordHandle`], [`RecvHandle`], [`SendHandle`] —
//! each run on a dedicated thread (or in the Sim/main app for `CoordHandle`).
//! [`PipelinedServer`] is the unified consumer-facing handle that owns all
//! three and exposes the primary [`PipelinedServer::new`] + [`PipelinedServer::tick`]
//! API.
//!
//! Naming note: this module is `pipeline_actors` (not `pipeline_handles`)
//! to avoid collision with the existing `server::pipeline_handles` module
//! that defines [`RecvHandle`] / [`SendHandle`].

mod handles;
mod orchestration;
mod recv_helpers;
mod router;
mod send_state_view;
mod server_entity_converter;
#[allow(missing_docs)]
mod event_receiver;
mod runtime;
mod sim_pipeline;
mod snapshot_sender;
mod spawn;

pub use handles::CoordHandle;
pub use orchestration::{
    apply_recv_to_world, configure_entity_replication, run_with_world_server, split_world_server,
};
pub use recv_helpers::{drain_lifecycle, drain_tick_buffer, RecvLifecycleEvent};
pub use router::TickMessageRouter;
pub use send_state_view::SendStateView;
pub use server_entity_converter::ServerEntityConverter;
pub use event_receiver::{
    RecvConnectEvent, RecvDespawnEntityEvent, RecvDisconnectEvent, RecvErrorEvent, EventReceiver,
    RecvPublishEntityEvent, RecvSpawnEntityEvent, RecvTickEvent, RecvUnpublishEntityEvent,
};
pub use runtime::{PipelineRuntime, RuntimeState, RuntimeTimingHooks};
pub use sim_pipeline::{PipelinedServer, TickCtx};
pub use snapshot_sender::{SnapshotReceiver, SnapshotSender};
pub use spawn::spawn_server_handles;

#[cfg(test)]
mod tests;
