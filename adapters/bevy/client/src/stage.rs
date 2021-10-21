use bevy::ecs::schedule::StageLabel;

#[derive(Debug, Clone, PartialEq, Eq, Hash, StageLabel)]
pub enum ClientStage {
    BeforeReceiveEvents,
}
