use bevy::prelude::{Component, Entity};

#[derive(Component)]
pub struct Line {
    pub start_entity: Entity,
    pub end_entity: Option<Entity>,
}

impl Line {
    pub fn new(start_entity: Entity, end_entity: Option<Entity>) -> Self {
        Self {
            start_entity,
            end_entity,
        }
    }
}
