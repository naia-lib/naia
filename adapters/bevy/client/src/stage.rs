use bevy_ecs::schedule::StageLabel;

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum Stage {
    Connection,
    Disconnection,
    ReceiveEvents,
    Tick,
    PreFrame,
    Frame,
    PostFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum PrivateStage {
    BeforeReceiveEvents,
    AfterTick,
    AfterConnection,
    AfterDisconnection,
}
