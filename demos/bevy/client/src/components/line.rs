use bevy::prelude::{Component, Entity};

#[derive(Component)]
pub struct Line {
    pub start_entity: Entity,
    pub end_entity: Entity,
    pub visible: bool,
}

impl Line {
    pub fn new(start_entity: Entity, end_entity: Entity) -> Self {
        Self {
            start_entity,
            end_entity,
            visible: true,
        }
    }
}
