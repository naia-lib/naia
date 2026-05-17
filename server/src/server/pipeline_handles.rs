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

use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use naia_shared::{Instant, Tick};

use crate::{
    connection::RecvConnection,
    server::{receive_output::ReceiveOutput, recv_state::RecvState, send_state::SendState},
};

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

    /// Pipeline-mode recv step (step 4-F.naia.c.2b).
    ///
    /// Drives the recv-only socket loop (no `SendState` access) and
    /// packages the per-tick handoff queues into a [`ReceiveOutput`] for
    /// the coordinator to thread into:
    ///   1. `WorldServer::drain_pending_handshakes` (coord-stage; needs
    ///      `coord.user_store`).
    ///   2. `SendHandle::process_recv_packets` (which consumes
    ///      `received_addresses` + `pending_data_packets` alongside a
    ///      `&mut` borrow of the recv connection map for the tick-buffer
    ///      decode + per-address ack drain + command finalization).
    pub fn receive(&mut self) -> ReceiveOutput<E> {
        self.state.receive();
        let world_events = self.state.take_world_events();
        let mut tick_events = self.state.take_tick_events(&Instant::now());
        let pending_ticks: Vec<Tick> =
            tick_events.read::<crate::events::TickEvent>().collect();
        let received_addresses = std::mem::take(&mut self.state.received_addresses);
        let pending_data_packets = std::mem::take(&mut self.state.pending_data_packets);
        ReceiveOutput {
            world_events,
            pending_ticks,
            received_addresses,
            pending_data_packets,
        }
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

    /// Pipeline-mode coord-stage cross-half processing (step 4-F.naia.c.2b).
    ///
    /// Thin wrapper that forwards to [`SendState::process_recv_packets`].
    /// The coordinator pulls `received_addresses` + `pending_data_packets`
    /// off a [`ReceiveOutput`] returned by [`RecvHandle::receive`] and
    /// passes `&mut recv_handle.state.recv_user_connections` for the
    /// tick-buffer decode borrow.
    pub fn process_recv_packets(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
        output: &mut ReceiveOutput<E>,
        server_tick: Tick,
    ) {
        let received_addresses = std::mem::take(&mut output.received_addresses);
        let pending_data_packets = std::mem::take(&mut output.pending_data_packets);
        self.state.process_recv_packets(
            recv_conns,
            received_addresses,
            pending_data_packets,
            server_tick,
        );
    }
}
