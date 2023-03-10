use bevy_ecs::schedule::SystemSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ReceiveEvents;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BeforeReceiveEvents;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct HostSyncChangeTracking;
