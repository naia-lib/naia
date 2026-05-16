use std::hash::Hash;

use naia_shared::Tick;

use crate::events::world_events::WorldEvents;

/// Decoded receive-phase output returned by [`WorldServer::receive`].
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
    /// Populated by [`WorldServer::receive`] via `take_tick_events`. The
    /// pipeline coordinator uses these to drive simulation work; the bevy
    /// adapter's `apply_receive_output` fires one `TickEvent` per entry.
    pub pending_ticks: Vec<Tick>,
}
