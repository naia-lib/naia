//! `spawn_server_handles` — single entry point that constructs a
//! [`WorldServer`] and splits it into a [`SimPipeline`].
//!
//! The returned [`SimPipeline`] owns all three sub-handles (sim, recv, send)
//! and exposes them through the [`SimPipeline::tick`] API.

use std::{hash::Hash, sync::Arc};

use naia_shared::Protocol;

use crate::{server::ServerShared, ServerConfig, WorldServer};

use super::{handles::SimHandle, sim_pipeline::SimPipeline};

/// Construct a [`WorldServer<E>`] and immediately split it into a
/// [`SimPipeline<E>`].
///
/// The `SimPipeline` owns all three pipeline sub-handles. Pass it to the
/// bevy adapter's `Plugin::sim_integration_full` via a `SimPipelineRes`
/// resource, then drive ticks with [`SimPipeline::tick`].
///
/// Equivalent to:
/// ```ignore
/// let ws = WorldServer::<E>::new(config, protocol);
/// let (coord_state, recv, send) = ws.into_pipeline_handles();
/// let shared = Arc::clone(&recv.state.shared);
/// let sim = SimHandle { state: coord_state, shared };
/// SimPipeline::new(sim, recv, send)
/// ```
pub fn spawn_server_handles<E, P>(
    server_config: ServerConfig,
    protocol: P,
) -> SimPipeline<E>
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    P: Into<Protocol>,
{
    let ws = WorldServer::<E>::new(server_config, protocol);
    let (coord_state, recv, send) = ws.into_pipeline_handles();
    let shared: Arc<ServerShared<E>> = Arc::clone(&recv.state.shared);
    let sim_handle = SimHandle {
        state: coord_state,
        shared,
    };
    SimPipeline::new(sim_handle, recv, send)
}
