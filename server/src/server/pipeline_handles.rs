//! Pipeline-mode handles for the recv and send halves of a `WorldServer`
//! (step 4-E.2f).
//!
//! Each handle owns its substate directly — no `Arc<Mutex<WorldServer>>`
//! wrapper. `RecvState<E>` and `SendState<E>` are both already `Send`
//! (their `unsafe impl Send` blocks at `recv_state.rs:96` and
//! `send_state.rs:48` carry the safety story), so the handles inherit
//! `Send` without any further unsafe.
//!
//! `WorldServer::into_pipeline_handles(self)` decomposes the server into
//! three pieces — `CoordinatorState<E>`, `RecvHandle<E>`, `SendHandle<E>` —
//! and `WorldServer::from_pipeline_states(...)` reassembles them. The
//! split is structural; thread-independent operation of the recv and
//! send halves is wired up by the bevy pipeline coordinator in step 4-F.
//!
//! Until 4-F, callers reassemble the handles back into a `WorldServer`
//! to drive `receive_all_packets` / `send_all_packets` through the
//! existing methods. The split is therefore a no-op for current
//! consumers, but the type-level rearrangement is in place so 4-F can
//! distribute the methods across threads without further public-API
//! changes.

use std::hash::Hash;

use crate::server::{recv_state::RecvState, send_state::SendState};

/// Recv-thread handle. Owns `RecvState<E>` directly.
///
/// In 4-F this handle is moved onto the recv thread; the coordinator
/// keeps `CoordinatorState<E>` and the `Arc<ServerShared<E>>` so the
/// three pieces can run concurrently behind the coordinator-driven
/// 12-step tick sequence (§8 line 1235 onwards).
pub struct RecvHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Recv-thread-exclusive substate (timers, queues, recv-side
    /// connection halves, pending data packets, recv io).
    pub state: RecvState<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> RecvHandle<E> {
    /// Consume this handle and return the inner `RecvState<E>` —
    /// used by `WorldServer::from_pipeline_states` for serial-mode
    /// reassembly until 4-F's coordinator runs the recv thread directly.
    pub fn into_state(self) -> RecvState<E> {
        self.state
    }
}

/// Send-thread handle. Owns `SendState<E>` directly.
pub struct SendHandle<E: Copy + Eq + Hash + Send + Sync> {
    /// Send-thread-exclusive substate (per-user send connections,
    /// per-user + global priority layers, send io).
    pub state: SendState<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> SendHandle<E> {
    /// Consume this handle and return the inner `SendState<E>` —
    /// used by `WorldServer::from_pipeline_states` for serial-mode
    /// reassembly until 4-F's coordinator runs the send thread directly.
    pub fn into_state(self) -> SendState<E> {
        self.state
    }
}
