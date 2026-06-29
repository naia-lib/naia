//! Backward-compat re-export: `spawn_server_handles` delegates to
//! [`PipelinedServer::new`].
//!
//! Prefer calling `PipelinedServer::new(config, protocol)` directly.

use std::hash::Hash;

use naia_shared::Protocol;

use crate::ServerConfig;

use super::sim_pipeline::PipelinedServer;

/// Construct a [`PipelinedServer<E>`]. Prefer `PipelinedServer::new` directly.
///
/// Kept for call-sites that haven't migrated yet; delegates directly to
/// [`PipelinedServer::new`] with identical semantics.
pub fn spawn_server_handles<E, P>(
    server_config: ServerConfig,
    protocol: P,
) -> PipelinedServer<E>
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    P: Into<Protocol>,
{
    PipelinedServer::new(server_config, protocol)
}
