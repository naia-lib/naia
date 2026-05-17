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
//! # Status (2026-05-16, post cyberlith.d)
//!
//! The cyberlith side `GameCell::update` is in serial-equivalent mode
//! (cyberlith `2da625e1`) and currently does NOT call
//! `into_pipeline_handles` — it keeps `WorldServer` reassembled and
//! drives `receive_with_world` + `send_all_packets` inline. The
//! multi-thread pipeline (step 4-F.cyberlith.e) is the next milestone.
//!
//! # Architectural constraints relevant to step 4-F.cyberlith.e
//!
//! **C1 (cross-half access).** `SendHandle::process_recv_packets` below
//! takes `&mut recv_conns` (from `RecvState::recv_user_connections`).
//! In a fully-3-threaded design those connections live on the recv
//! thread and `SendState` lives in coord/send territory — no single
//! thread holds both, so this method cannot be called in true 3-thread
//! mode. The .e MVP keeps recv on the coord (single thread holds both
//! halves) and parallelises only sim + send.
//!
//! **C2 (World mutation).** The decoded entity events (spawn / insert /
//! despawn) are applied to `&mut World` by
//! `WorldServer::process_all_packets`, which mutates `self.send.*`
//! while doing so. After `into_pipeline_handles`, the coord no longer
//! holds a unified `WorldServer`. The .e MVP works around this by NOT
//! calling `into_pipeline_handles` — it uses `receive_with_world`
//! (cyberlith.d) which bundles `receive_all_packets +
//! process_all_packets` inline.
//!
//! **C3 (send extraction).** `SendHandle::send_all_packets` does not
//! exist yet. Adding it is step 4-F.naia.h — see the doc-comment on
//! `WorldServer::send_all_packets` for the three-phase factoring plan.
//!
//! See `cyberlith/_AGENTS/MISSION_CAPACITY_UPLIFT.md` ("4-F.cyberlith.e
//! — multi-thread pipeline coordinator", architectural reality check
//! C1/C2/C3/C4) for the design decisions these constraints drove.

use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use naia_shared::{Instant, Tick, WorldRefType};

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

    /// Pipeline-mode periodic ping dispatch (step 4-F.naia.c.2c).
    /// Thin wrapper that forwards to [`SendState::send_pings`].
    pub fn send_pings(
        &mut self,
        recv_conns: &mut HashMap<SocketAddr, RecvConnection>,
    ) {
        self.state.send_pings(recv_conns);
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

    /// Pipeline-mode send-half tick body (step 4-F.naia.h).
    ///
    /// Thin wrapper that forwards to [`SendState::send_all_packets`].
    /// Called by the 4-F.cyberlith.e coordinator on the send thread AFTER
    /// the coord thread has called `WorldServer::run_send_preamble` on
    /// the reassembled server (or, once the coord/send split is taken
    /// further, after a coord-side equivalent of the preamble runs).
    ///
    /// The world snapshot passed in must outlive the call. Today the
    /// only `WorldRefType + Sync` implementation that crosses thread
    /// boundaries is bevy's `world.proxy()`; if cyberlith.e finds it
    /// is not actually `Send`-safe to move across threads, the send
    /// thread will instead need a cheap clone of the relevant subset
    /// before this call. The signature here matches the serial
    /// `SendState::send_all_packets` shape exactly so the two paths
    /// remain interchangeable.
    pub fn send_all_packets<W: WorldRefType<E> + Sync>(&mut self, world: W) {
        self.state.send_all_packets(world);
    }
}
