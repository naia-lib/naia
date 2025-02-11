use std::{collections::HashMap, hash::Hash};

use crate::{BigMap, EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity};

pub struct GlobalEntityMap<E: Copy + Eq + Hash + Send + Sync> {
    entity_to_global_map: HashMap<E, GlobalEntity>,
    global_to_entity_map: BigMap<GlobalEntity, E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalEntityMap<E> {
    pub fn new() -> Self {
        Self {
            entity_to_global_map: HashMap::new(),
            global_to_entity_map: BigMap::new(),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E> for GlobalEntityMap<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        match self.global_to_entity_map.get(global_entity) {
            Some(world_entity) => Ok(*world_entity),
            None => Err(EntityDoesNotExistError),
        }
    }

    fn entity_to_global_entity(&self, world_entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match self.entity_to_global_map.get(world_entity) {
            Some(global_entity) => Ok(*global_entity),
            None => Err(EntityDoesNotExistError),
        }
    }
}

pub trait GlobalEntitySpawner<E: Copy + Eq + Hash + Send + Sync>: EntityAndGlobalEntityConverter<E> {
    fn spawn(&mut self, world_entity: E) -> GlobalEntity;
    fn despawn_by_global(&mut self, global_entity: GlobalEntity);
    fn despawn_by_world(&mut self, world_entity: E);
    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E>;
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalEntitySpawner<E> for GlobalEntityMap<E> {
    fn spawn(&mut self, world_entity: E) -> GlobalEntity {

        let global_entity = self.global_to_entity_map.insert(world_entity);
        self.entity_to_global_map.insert(world_entity, global_entity);

        global_entity
    }

    fn despawn_by_global(&mut self, global_entity: GlobalEntity) {
        let world_entity = self.global_to_entity_map.remove(&global_entity).unwrap();
        self.entity_to_global_map.remove(&world_entity);
    }

    fn despawn_by_world(&mut self, world_entity: E) {
        let global_entity = self.entity_to_global_map.remove(&world_entity).unwrap();
        self.global_to_entity_map.remove(&global_entity);
    }

    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }
}