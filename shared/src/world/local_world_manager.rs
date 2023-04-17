use std::{collections::HashMap, hash::Hash};

use crate::world::entity::owned_entity::OwnedEntity;
use crate::{EntityDoesNotExistError, KeyGenerator, LocalEntity, LocalEntityConverter};

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    host_entity_generator: KeyGenerator<LocalEntity>,
    world_to_local_entity: HashMap<E, OwnedEntity>,
    local_to_world_entity: HashMap<OwnedEntity, E>,
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

    pub(crate) fn host_spawn_entity(&mut self, entity: &E) {
        let host_owned_entity = self.host_entity_generator.generate().to_host_owned();

        if self.world_to_local_entity.contains_key(entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&host_owned_entity) {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity
            .insert(*entity, host_owned_entity);
        self.local_to_world_entity
            .insert(host_owned_entity, *entity);
    }

    pub(crate) fn host_despawn_entity(&mut self, entity: &E) {
        let owned_entity = self
            .world_to_local_entity
            .remove(entity)
            .expect("Entity does not exist!");
        if !self.local_to_world_entity.contains_key(&owned_entity) {
            panic!("Net Entity does not exist!");
        }
        self.local_to_world_entity.remove(&owned_entity);
        self.host_entity_generator
            .recycle_key(&owned_entity.to_unowned());
    }

    // Remote entities

    pub(crate) fn get_remote_entity(&self, local_entity: &LocalEntity) -> E {
        let remote_owned_entity = local_entity.to_remote_owned();

        if let Some(world_entity) = self.local_to_world_entity.get(&remote_owned_entity) {
            return *world_entity;
        }
        panic!("Attempting to access remote entity which does not exist!")
    }

    pub(crate) fn remote_entities(&self) -> Vec<E> {
        let mut output = Vec::new();
        for (owned_entity, entity) in &self.local_to_world_entity {
            if owned_entity.is_host() {
                continue;
            }
            output.push(*entity);
        }
        return output;
    }

    pub(crate) fn remote_spawn_entity(&mut self, entity: &E, local_entity: &LocalEntity) {
        let remote_owned_entity = local_entity.to_remote_owned();

        if self.world_to_local_entity.contains_key(entity) {
            panic!("Entity already exists!");
        }
        if self
            .local_to_world_entity
            .contains_key(&remote_owned_entity)
        {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity
            .insert(*entity, remote_owned_entity);
        self.local_to_world_entity
            .insert(remote_owned_entity, *entity);
    }

    pub(crate) fn remote_despawn_entity(&mut self, local_entity: &LocalEntity) -> E {
        let remote_owned_entity = local_entity.to_remote_owned();

        if let Some(world_entity) = self.local_to_world_entity.remove(&remote_owned_entity) {
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
    fn entity_to_local_entity(&self, entity: &E) -> Result<OwnedEntity, EntityDoesNotExistError> {
        if let Some(owned_entity) = self.world_to_local_entity.get(entity) {
            return Ok(*owned_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn local_entity_to_entity(
        &self,
        owned_entity: &OwnedEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.local_to_world_entity.get(owned_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }
}
