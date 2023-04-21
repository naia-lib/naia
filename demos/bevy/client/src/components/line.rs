use bevy::prelude::{Component, Entity};

#[derive(Component)]
pub struct Line {
    pub start_entity: Entity,
    pub end_entity: Entity,
}

impl Line {
    pub fn new(square: Entity,
               baseline: Entity) -> Self {
        Self {
            start_entity: square,
            end_entity: baseline,
        }
    }
}
