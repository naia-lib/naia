//! Backward-compat re-export: `spawn_server_handles` delegates to
//! [`PipelinedWorldServer::new`].
//!
//! Prefer calling `PipelinedWorldServer::new(config, protocol)` directly.

use std::hash::Hash;

use naia_shared::Protocol;

use crate::ServerConfig;

use super::sim_pipeline::PipelinedWorldServer;

/// Construct a [`PipelinedWorldServer<E>`]. Prefer `PipelinedWorldServer::new` directly.
///
/// Kept for call-sites that haven't migrated yet; delegates directly to
/// [`PipelinedWorldServer::new`] with identical semantics.
pub fn spawn_server_handles<E, P>(
    server_config: ServerConfig,
    protocol: P,
) -> PipelinedWorldServer<E>
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    P: Into<Protocol>,
{
    PipelinedWorldServer::new(server_config, protocol)
}
