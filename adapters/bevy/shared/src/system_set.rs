use bevy_ecs::schedule::SystemSet;

// internal to Bevy adapter crates
/// System set for naia's packet-receive system.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReceivePackets;

// internal to Bevy adapter crates
/// System set for naia's incoming-packet-processing systems.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ProcessPackets;

// internal to Bevy adapter crates
/// System set that translates naia tick events into their Bevy form, ahead
/// of [`HandleTickEvents`].
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct TranslateTickEvents;

// for use by apps using Bevy adapter crates
/// System set apps schedule their own systems against to run after naia's
/// tick events have been translated.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HandleTickEvents;

// internal to Bevy adapter crates
/// System set that translates naia world events into their Bevy form, ahead
/// of [`HandleWorldEvents`].
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct TranslateWorldEvents;

// for use by apps using Bevy adapter crates
/// System set apps schedule their own systems against to run after naia's
/// world events have been translated.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HandleWorldEvents;

// internal to Bevy adapter crates
/// System set for naia's world-update systems.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WorldUpdate;

// internal to Bevy adapter crates
/// System set for [`crate::on_host_owned_added`], which must run before
/// [`HostSyncChangeTracking`].
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncOwnedAddedTracking;

// internal to Bevy adapter crates
/// System set for the per-component `on_component_added` / `on_component_removed`
/// and `on_despawn` host-sync systems.
///
/// Public contract: consumers order their own systems against this whole set
/// (`.before(...)` / `.after(...)`) without naming naia internals. Inside the
/// set, members run as one explicitly ordered sequence: each component kind's
/// added-then-removed pair in protocol registration order (see
/// [`HostSyncKindSlot`]), with `on_despawn` last (see
/// [`HostSyncDespawnTrailer`]).
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncChangeTracking;

// internal to Bevy adapter crates
/// Ordering slot for one component kind's `on_component_added` /
/// `on_component_removed` pair inside [`HostSyncChangeTracking`].
///
/// Slot `n` holds the pair for the `n`-th component kind in protocol
/// registration order (the `ComponentKinds` net-ID order, which already has
/// to match on both ends of the wire). `WorldData` chains consecutive slots
/// and the [`HostSyncDespawnTrailer`] behind them, so the trackers execute
/// in a deterministic order that is identical in every process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostSyncKindSlot(pub u32);

impl SystemSet for HostSyncKindSlot {
    fn dyn_clone(&self) -> Box<dyn SystemSet> {
        Box::new(*self)
    }
}

// internal to Bevy adapter crates
/// Trailing slot inside [`HostSyncChangeTracking`]: `on_despawn` joins this
/// slot, and `WorldData` orders it after the last [`HostSyncKindSlot`], so
/// despawn bookkeeping always runs after every per-kind tracker.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncDespawnTrailer;

// internal to Bevy adapter crates
/// System set for systems that push host-owned world changes into naia's
/// replication state.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WorldToHostSync;

/// System set for naia's packet-send system; metrics plugins and other
/// per-tick consumers schedule themselves after this set.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SendPackets;
