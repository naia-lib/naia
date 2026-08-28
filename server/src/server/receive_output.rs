use std::{collections::HashSet, hash::Hash, net::SocketAddr};

use naia_shared::{OwnedBitReader, Tick};

use crate::events::world_events::WorldEvents;

/// Decoded receive-phase output returned by [`InternalWorldServer::receive`].
///
/// Contains everything accumulated during the recv phase in plain-data form
/// that can cross thread boundaries, for use in the pipeline coordinator.
///
/// The companion [`apply_receive_output`] (in `naia_bevy_server`) fires the
/// same Bevy events that naia has always fired from these decoded outputs.
///
/// # Phase 4
///
/// Adds `pending_ticks: Vec<Tick>`. In the Phase 4 pipeline the recv thread
/// owns tick-clock advancement (`take_tick_events`) and the resulting tick
/// IDs are delivered alongside world events; `apply_receive_output` fires
/// one Bevy `TickEvent` per pending tick.
///
/// [`apply_receive_output`]: https://docs.rs/naia_bevy_server
pub struct ReceiveOutput<E: Copy + Eq + Hash + Send + Sync> {
    /// Events decoded from the receive phase (Connect, Disconnect, Messages, …).
    ///
    /// Re-uses the existing [`WorldEvents<E>`] type which is already `Send`.
    pub world_events: WorldEvents<E>,

    /// Server ticks that fired during this receive phase.
    ///
    /// Populated by [`InternalWorldServer::receive`] via `take_tick_events`. The
    /// pipeline coordinator uses these to drive simulation work; the bevy
    /// adapter's `apply_receive_output` fires one `TickEvent` per entry.
    pub pending_ticks: Vec<Tick>,

    /// Addresses that delivered at least one packet during this recv phase
    /// (step 4-F.naia.c.2b). Populated by `RecvHandle::receive` /
    /// `InternalWorldServer::receive_all_packets`; consumed by
    /// `SendHandle::process_recv_packets` for the per-address `drain_acks` +
    /// `process_received_commands` sweep. The set is intentionally
    /// broader than the data-only `addrs_with_new_packets` set on
    /// `RecvState`: any inbound Heartbeat / Ping / Pong / Data counts.
    pub received_addresses: HashSet<SocketAddr>,

    /// Data packets buffered during the recv phase, drained from
    /// `RecvState::pending_data_packets` (step 4-F.naia.c.2b). Each tuple
    /// is `(addr, client_tick, owned_reader)` — the reader has already
    /// advanced past `StandardHeader` + `Tick`, so the decoder starts at
    /// the message/world section directly.
    ///
    /// In serial mode `InternalWorldServer::receive_all_packets` packages these
    /// into the `ReceiveOutput` it constructs internally and passes them
    /// to `SendState::process_recv_packets` inline. In pipeline mode the
    /// coordinator drains them off the returned `ReceiveOutput` and hands
    /// them to `SendHandle::process_recv_packets` alongside the recv
    /// connection map.
    pub pending_data_packets: Vec<(SocketAddr, Tick, OwnedBitReader)>,
}

impl<E: Copy + Eq + Hash + Send + Sync> ReceiveOutput<E> {
    /// Returns true if this `ReceiveOutput` contains no events, ticks,
    /// received addresses, or pending data packets. An empty output is
    /// safe to skip in the cross-half orchestration pipeline.
    pub fn is_empty(&self) -> bool {
        self.pending_ticks.is_empty()
            && self.received_addresses.is_empty()
            && self.pending_data_packets.is_empty()
            && self.world_events.is_empty()
    }
}
