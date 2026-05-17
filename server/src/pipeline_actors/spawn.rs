//! `spawn_server_handles` — single entry point that constructs a
//! [`WorldServer`] and immediately splits it into the three pipeline
//! handles (coord + recv + send).
//!
//! Cyberlith's `GameCell::init` (per MISSION_SIM_OWNS_WORLD.md Phase B.7
//! / C.1 / D.1) calls this once at startup; the three returned handles
//! are then handed off to the Recv / Sim / Send SubApps.

use std::{hash::Hash, sync::Arc};

use naia_shared::Protocol;

use crate::{
    RecvHandle, SendHandle, ServerConfig, WorldServer,
    server::ServerShared,
};

use super::CoordHandle;

/// Construct a [`WorldServer<E>`] and immediately split it into the three
/// pipeline handles consumed by the 3-SubApp architecture.
///
/// Equivalent to:
/// ```ignore
/// let ws = WorldServer::<E>::new(config, protocol);
/// let (coord_state, recv, send) = ws.into_pipeline_handles();
/// let shared = Arc::clone(&recv.state.shared);
/// (CoordHandle { state: coord_state, shared }, recv, send)
/// ```
///
/// The returned `Arc<ServerShared<E>>` on [`CoordHandle::shared`] is a
/// clone of the same `Arc` already held by `recv.state.shared` and
/// `send.state.shared` — all three handles see the same underlying
/// [`ServerShared`] allocation, so cross-handle reads of shared init
/// data are consistent without any synchronization beyond what
/// [`ServerShared`] itself provides.
pub fn spawn_server_handles<E, P>(
    server_config: ServerConfig,
    protocol: P,
) -> (CoordHandle<E>, RecvHandle<E>, SendHandle<E>)
where
    E: Copy + Eq + Hash + Send + Sync + 'static,
    P: Into<Protocol>,
{
    let ws = WorldServer::<E>::new(server_config, protocol);
    let (coord_state, recv, send) = ws.into_pipeline_handles();
    let shared: Arc<ServerShared<E>> = Arc::clone(&recv.state.shared);
    let coord = CoordHandle { state: coord_state, shared };
    (coord, recv, send)
}
