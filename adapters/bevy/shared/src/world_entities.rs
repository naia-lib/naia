use std::{
    collections::HashSet,
    default::Default,
};

use bevy_ecs::{
    entity::Entity,
    prelude::Resource,
};

#[derive(Resource)]
pub struct WorldEntities {
    entities: HashSet<Entity>,
}

unsafe impl Send for WorldEntities {}
unsafe impl Sync for WorldEntities {}

impl Default for WorldEntities {
    fn default() -> Self {
        Self {
            entities: HashSet::default(),
        }
    }
}

impl WorldEntities {

    // Entities //

    pub fn entities(&self) -> Vec<Entity> {
        let mut output = Vec::new();

        for entity in &self.entities {
            output.push(*entity);
        }

        output
    }

    pub fn spawn_entity(&mut self, entity: &Entity) {
        self.entities.insert(*entity);
    }

    pub fn despawn_entity(&mut self, entity: &Entity) {
        self.entities.remove(entity);
    }
}
