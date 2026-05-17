//! `CoordHandle<E>` — bundles the coordinator-thread state with a clone
//! of the cross-thread shared init (`Arc<ServerShared<E>>`) so cyberlith's
//! Recv SubApp can install it as a single resource.
//!
//! The `Arc<ServerShared<E>>` clone here is independent of (but identical
//! to) the clones living on `RecvHandle::state.shared` and
//! `SendHandle::state.shared` — all three point at the same underlying
//! [`ServerShared`] allocation.

use std::{hash::Hash, sync::Arc};

use crate::server::ServerShared;
use crate::server::coord_state::CoordinatorState;

/// Coord-half of a pipeline-mode [`crate::WorldServer`].
///
/// Holds [`CoordinatorState`] (`user_store`, `room_store`, scope tables,
/// historian, etc.) plus a clone of the cross-thread `Arc<ServerShared>`
/// so the coord-side code can read shared init (`server_config`, kind
/// tables, `global_dirty`) without going through the recv or send handle.
pub struct CoordHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Coordinator-thread state. Single-threaded — no internal locking.
    pub state: CoordinatorState<E>,
    /// Cross-thread shared init + atomic cells. Cloneable `Arc` — the
    /// same allocation is also referenced by [`crate::RecvHandle::state.shared`]
    /// and [`crate::SendHandle::state.shared`].
    pub shared: Arc<ServerShared<E>>,
}
