use bevy::ecs::schedule::StageLabel;

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum Stage {
    Connection,
    Disconnection,
    ReceiveEvents,
    PreFrame,
    Frame,
    PostFrame,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum PrivateStage {
    AfterTick,
    AfterConnection,
    AfterDisconnection,
}
