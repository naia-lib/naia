use std::hash::Hash;

use crate::events::world_events::WorldEvents;

/// Decoded receive-phase output returned by [`WorldServer::receive`].
///
/// Contains everything accumulated during the recv phase in plain-data form
/// that can cross thread boundaries, for use in the pipeline coordinator.
///
/// The companion [`apply_receive_output`] (in `naia_bevy_server`) fires the
/// same Bevy events that naia has always fired from these decoded outputs.
///
/// # Phase 3 note
///
/// For Phase 3 this is a thin wrapper around [`WorldEvents<E>`] — the
/// existing event accumulator that is already `Send`.  The field-level split
/// required for true concurrent recv/send execution is deferred to Phase 4.
///
/// [`apply_receive_output`]: https://docs.rs/naia_bevy_server
pub struct ReceiveOutput<E: Copy + Eq + Hash + Send + Sync> {
    /// Events decoded from the receive phase (Connect, Disconnect, Messages, …).
    ///
    /// Re-uses the existing [`WorldEvents<E>`] type which is already `Send`.
    pub world_events: WorldEvents<E>,
}
