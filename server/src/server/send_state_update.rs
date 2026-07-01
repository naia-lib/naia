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
//! Global-entity priority does NOT travel through this queue: the
//! `global_priority` move uses **publish-on-read** at the start of
//! `send_all_packets` — `sim_handle.global_priority_mirror` is cloned
//! wholesale into `send.global_priority` each tick. That is the single,
//! source-agnostic mechanism; it stays correct even if the coordinator
//! and send threads diverge (only the clone becomes redundant, never
//! wrong). A per-entity push path was considered and rejected: at
//! realistic churn it is break-even-to-slower than the bulk clone and
//! adds a per-mutation-site enqueue obligation the byte-exact moat can't
//! afford to have silently missed.

use std::net::SocketAddr;

use crate::connection::SendConnection;

/// Recv-side → send-side handoff event.
///
/// See module docs for variant intent.
pub enum SendStateUpdate {
    /// A new connection was finalized on the recv thread; insert the
    /// matching `SendConnection` into `SendState.send_user_connections`.
    ConnectionAdded(SocketAddr, Box<SendConnection>),

    /// A connection was torn down on the coordinator thread; drop the
    /// matching `SendConnection` from `SendState.send_user_connections`.
    ConnectionRemoved(SocketAddr),
}
