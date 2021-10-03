use bevy::ecs::schedule::StageLabel;

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum ServerStage {
    ServerEvents,
    Tick,
    UpdateScopes,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum PrivateStage {
    ReadEvents,
    SendPackets,
    // these are here to flush any ServerCommands
    AfterEvents,
    AfterUpdate,
    AfterTick,
}
