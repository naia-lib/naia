//! Recv-side → send-side handoff queue (C.3 Phase 4 step 4-E.2e).
//!
//! `SendStateUpdate<E>` is the only path through which mutations made on
//! the recv / coordinator threads reach `SendState<E>` in pipeline mode.
//! The queue lives at `ServerShared::pending_send_state_updates` (LOCK
//! ORDER position #5).
//!
//! In serial mode, the queue is drained inline at the tail of
//! `InternalWorldServer::receive`. In pipeline mode (step 4-F), the coordinator
//! drains it at step 6.5 between the recv and send phases of the tick.
//!
//! ## Variants
//!
//! * `ConnectionAdded(addr, send_conn)` — pushed from `finalize_connection`
//!   which runs on the recv thread. The `SendConnection` half can't be
//!   inserted into `SendState.send_user_connections` directly from the
//!   recv thread (that map is send-thread-exclusive in pipeline mode), so
//!   it travels through the queue.
//!
//! * `ConnectionRemoved(addr)` — pushed from the disconnect path. The
//!   apply step drops the matching `SendConnection` from
//!   `send_user_connections`. The recv side and the coordinator-owned
//!   `shared.connection_shared` map are torn down directly on the
//!   coordinator thread (it owns those).
//!
//! * `PriorityChanged { entity, gain }` — **defined but unused in
//!   4-E.2e.** The borrow API (`global_entity_priority_mut`) returns
//!   `EntityPriorityMut<'_, E>` which holds `&mut HashMap` from
//!   `naia-shared`; routing every mutation through this queue would
//!   require breaking changes to that public type. The 4-E.2e
//!   `global_priority` move uses **publish-on-read** at the start of
//!   `send_all_packets` instead — `sim_handle.global_priority_mirror` is
//!   cloned into `send.global_priority`. The `PriorityChanged` variant
//!   exists so a future commit (likely 4-F when the coordinator and
//!   send threads truly diverge) can rewire the borrow API to push
//!   per-entity updates here without needing a second enum migration.

use std::{hash::Hash, net::SocketAddr};

use crate::connection::SendConnection;

/// Recv-side → send-side handoff event.
///
/// See module docs for variant intent and the publish-on-read note on
/// `PriorityChanged`.
#[allow(dead_code)]
pub enum SendStateUpdate<E: Copy + Eq + Hash + Send + Sync> {
    /// A new connection was finalized on the recv thread; insert the
    /// matching `SendConnection` into `SendState.send_user_connections`.
    ConnectionAdded(SocketAddr, Box<SendConnection>),

    /// A connection was torn down on the coordinator thread; drop the
    /// matching `SendConnection` from `SendState.send_user_connections`.
    ConnectionRemoved(SocketAddr),

    /// Future-use placeholder (see module docs). The 4-E.2e
    /// `global_priority` move uses publish-on-read; this variant is
    /// reserved for when the borrow API is rewired to push per-entity
    /// updates through the queue.
    ///
    /// `gain == None` means "clear the override" (revert to the default
    /// 1.0); `Some(g)` sets an explicit gain.
    PriorityChanged { entity: E, gain: Option<f32> },
}
