use std::{collections::HashMap, hash::Hash};

use crate::world::entity::local_entity::LocalEntity;
use crate::{EntityDoesNotExistError, KeyGenerator, LocalEntityConverter};

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    host_entity_generator: KeyGenerator<u16>,
    world_to_local_entity: HashMap<E, LocalEntity>,
    local_to_world_entity: HashMap<LocalEntity, E>,
}

impl<E: Copy + Eq + Hash> LocalWorldManager<E> {
    pub fn new() -> Self {
        Self {
            host_entity_generator: KeyGenerator::new(),
            world_to_local_entity: HashMap::new(),
            local_to_world_entity: HashMap::new(),
        }
    }

    // Host entities

    pub(crate) fn host_spawn_entity(&mut self, world_entity: &E) {
        let host_entity = LocalEntity::Host(self.host_entity_generator.generate());

        if self.world_to_local_entity.contains_key(world_entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&host_entity) {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity
            .insert(*world_entity, host_entity);
        self.local_to_world_entity
            .insert(host_entity, *world_entity);
    }

    pub(crate) fn host_despawn_entity(&mut self, world_entity: &E) {
        let local_entity = self
            .world_to_local_entity
            .remove(world_entity)
            .expect("Entity does not exist!");
        if !self.local_to_world_entity.contains_key(&local_entity) {
            panic!("Net Entity does not exist!");
        }
        self.local_to_world_entity.remove(&local_entity);
        self.host_entity_generator
            .recycle_key(&local_entity.value());
    }

    // Remote entities

    pub(crate) fn get_remote_entity(&self, local_entity: &LocalEntity) -> E {
        if !local_entity.is_remote() {
            panic!("can only call this method with remote entities");
        }

        if let Some(world_entity) = self.local_to_world_entity.get(&local_entity) {
            return *world_entity;
        }
        panic!("Attempting to access remote entity which does not exist!")
    }

    pub(crate) fn remote_entities(&self) -> Vec<E> {
        let mut output = Vec::new();
        for (local_entity, entity) in &self.local_to_world_entity {
            if local_entity.is_host() {
                continue;
            }
            output.push(*entity);
        }
        return output;
    }

    pub(crate) fn remote_spawn_entity(&mut self, world_entity: &E, local_entity: &LocalEntity) {
        if !local_entity.is_remote() {
            panic!("can only call this method with remote entities");
        }

        if self.world_to_local_entity.contains_key(world_entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&local_entity) {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity
            .insert(*world_entity, *local_entity);
        self.local_to_world_entity
            .insert(*local_entity, *world_entity);
    }

    pub(crate) fn remote_despawn_entity(&mut self, local_entity: &LocalEntity) -> E {
        if !local_entity.is_remote() {
            panic!("can only call this method with remote entities");
        }

        if let Some(world_entity) = self.local_to_world_entity.remove(local_entity) {
            if self.world_to_local_entity.remove(&world_entity).is_none() {
                panic!("Entity already exists!");
            } else {
                return world_entity;
            }
        } else {
            panic!("Trying to despawn a remote entity that does not exist!");
        }
    }
}

impl<E: Copy + Eq + Hash> LocalEntityConverter<E> for LocalWorldManager<E> {
    fn entity_to_local_entity(&self, entity: &E) -> Result<LocalEntity, EntityDoesNotExistError> {
        if let Some(local_entity) = self.world_to_local_entity.get(entity) {
            return Ok(*local_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn local_entity_to_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.local_to_world_entity.get(local_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }
}
