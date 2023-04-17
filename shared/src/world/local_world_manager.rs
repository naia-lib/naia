use std::{collections::HashMap, hash::Hash};

use crate::{EntityDoesNotExistError, KeyGenerator, NetEntity, NetEntityConverter};
use crate::world::entity::owned_net_entity::OwnedNetEntity;

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    host_entity_generator: KeyGenerator<NetEntity>,
    world_to_local_entity: HashMap<E, OwnedNetEntity>,
    local_to_world_entity: HashMap<OwnedNetEntity, E>,
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
        let net_entity = self.host_entity_generator.generate().to_host_owned();

        if self.world_to_local_entity.contains_key(entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&net_entity) {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity.insert(*entity, net_entity);
        self.local_to_world_entity.insert(net_entity, *entity);
    }

    pub(crate) fn host_despawn_entity(&mut self, entity: &E) {
        let net_entity = self
            .world_to_local_entity
            .remove(entity)
            .expect("Entity does not exist!");
        if !self.local_to_world_entity.contains_key(&net_entity) {
            panic!("Net Entity does not exist!");
        }
        self.local_to_world_entity.remove(&net_entity);
        self.host_entity_generator
            .recycle_key(&net_entity.to_unowned());
    }

    // Remote entities

    pub(crate) fn get_remote_entity(&self, net_entity: &NetEntity) -> E {
        let owned_net_entity = net_entity.to_remote_owned();

        if let Some(world_entity) = self.local_to_world_entity.get(&owned_net_entity) {
            return *world_entity;
        }
        panic!("Attempting to access remote entity which does not exist!")
    }

    pub(crate) fn remote_entities(&self) -> Vec<E> {
        let mut output = Vec::new();
        for (owned_net_entity, entity) in &self.local_to_world_entity {
            if owned_net_entity.is_host() {
                continue;
            }
            output.push(*entity);
        }
        return output;
    }

    pub(crate) fn remote_spawn_entity(&mut self, entity: &E, net_entity: &NetEntity) {
        let owned_net_entity = net_entity.to_remote_owned();

        if self.world_to_local_entity.contains_key(entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&owned_net_entity) {
            panic!("Net Entity already exists!");
        }

        self.world_to_local_entity.insert(*entity, owned_net_entity);
        self.local_to_world_entity.insert(owned_net_entity, *entity);
    }

    pub(crate) fn remote_despawn_entity(&mut self, net_entity: &NetEntity) -> E {
        let owned_net_entity = net_entity.to_remote_owned();

        if let Some(world_entity) = self.local_to_world_entity.remove(&owned_net_entity) {
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

impl<E: Copy + Eq + Hash> NetEntityConverter<E> for LocalWorldManager<E> {
    fn entity_to_net_entity(&self, entity: &E) -> Result<OwnedNetEntity, EntityDoesNotExistError> {
        if let Some(net_entity) = self.world_to_local_entity.get(entity) {
            return Ok(*net_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn net_entity_to_entity(
        &self,
        net_entity: &OwnedNetEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.local_to_world_entity.get(net_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }
}
