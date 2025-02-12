use std::{
    collections::HashMap,
    default::Default,
};

use bevy_ecs::{
    prelude::Resource,
    world::World
};

use crate::world_entity::WorldId;

#[derive(Resource)]
pub struct SubWorlds {
    map: HashMap<WorldId, World>,
    next_world_id: u16,
}

unsafe impl Send for SubWorlds {}
unsafe impl Sync for SubWorlds {}

impl Default for SubWorlds {
    fn default() -> Self {
        Self {
            map: HashMap::default(),
            next_world_id: 0,
        }
    }
}

impl SubWorlds {

    pub fn init_world(&mut self) -> WorldId {
        let world_id = WorldId::sub(self.next_world_id);
        self.next_world_id += 1;

        self.map.insert(world_id, World::default());

        world_id
    }

    pub(crate) fn get_world(&self, world_id: &WorldId) -> &World {
        if world_id.is_main() {
            panic!("Attempted to get main world from SubWorlds");
        }

        self.map.get(world_id).unwrap()
    }

    pub(crate) fn get_world_mut(&mut self, world_id: &WorldId) -> &mut World {
        if world_id.is_main() {
            panic!("Attempted to get main world from SubWorlds");
        }

        self.map.get_mut(world_id).unwrap()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (WorldId, &mut World)> {
        self.map.iter_mut().map(|(world_id, world)| (*world_id, world))
    }
}
