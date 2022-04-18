use bevy_ecs::schedule::StageLabel;

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum Stage {
    ReceiveEvents,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum PrivateStage {
    BeforeReceiveEvents,
    AfterTick,
}
