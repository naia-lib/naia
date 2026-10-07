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
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncChangeTracking;

// internal to Bevy adapter crates
/// System set for systems that push host-owned world changes into naia's
/// replication state.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WorldToHostSync;

/// System set for naia's packet-send system; metrics plugins and other
/// per-tick consumers schedule themselves after this set.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SendPackets;
