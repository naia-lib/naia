use bevy_ecs::schedule::SystemSet;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReceivePackets;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ProcessPackets;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct TranslateTickEvents;

// for use by apps using Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HandleTickEvents;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct TranslateWorldEvents;

// for use by apps using Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HandleWorldEvents;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WorldUpdate;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncOwnedAddedTracking;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncChangeTracking;

// internal to Bevy adapter crates
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct WorldToHostSync;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct SendPackets;
