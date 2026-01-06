use std::{collections::HashMap, hash::Hash};

use crate::{
    BigMap, EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity, RemoteEntity,
};

pub struct GlobalEntityMap<E: Copy + Eq + Hash + Send + Sync> {
    entity_to_global_map: HashMap<E, GlobalEntity>,
    global_to_entity_map: BigMap<GlobalEntity, Option<E>>,
    reserved_global_entities: HashMap<RemoteEntity, GlobalEntity>,
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalEntityMap<E> {
    pub fn new() -> Self {
        Self {
            entity_to_global_map: HashMap::new(),
            global_to_entity_map: BigMap::new(),
            reserved_global_entities: HashMap::new(),
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E> for GlobalEntityMap<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        match self.global_to_entity_map.get(global_entity) {
            Some(world_entity_opt) => {
                if let Some(world_entity) = world_entity_opt {
                    return Ok(*world_entity);
                }
                // warn!(
                //     "Global entity {:?} exists but does not map to a world entity yet, it is reserved.",
                //     global_entity
                // );
                return Err(EntityDoesNotExistError);
            }
            None => Err(EntityDoesNotExistError),
        }
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match self.entity_to_global_map.get(world_entity) {
            Some(global_entity) => Ok(*global_entity),
            None => Err(EntityDoesNotExistError),
        }
    }
}

pub trait GlobalEntitySpawner<E: Copy + Eq + Hash + Send + Sync>:
    EntityAndGlobalEntityConverter<E>
{
    fn spawn(&mut self, world_entity: E, remote_entity_opt: Option<RemoteEntity>) -> GlobalEntity;
    fn reserve_global_entity(&mut self, remote_entity: RemoteEntity) -> GlobalEntity;
    fn despawn_by_global(&mut self, global_entity: &GlobalEntity);
    fn despawn_by_world(&mut self, world_entity: &E);
    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E>;
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalEntitySpawner<E> for GlobalEntityMap<E> {
    fn spawn(&mut self, world_entity: E, remote_entity_opt: Option<RemoteEntity>) -> GlobalEntity {
        let global_entity_opt;

        if let Some(remote_entity) = remote_entity_opt {
            if let Some(global_entity) = self.reserved_global_entities.remove(&remote_entity) {
                // global entity was reserved, update the mapping
                let Some(entry) = self.global_to_entity_map.get_mut(&global_entity) else {
                    panic!(
                        "Global entity {:?} does not exist in the global to entity map",
                        global_entity
                    );
                };
                *entry = Some(world_entity);
                global_entity_opt = Some(global_entity);
            } else {
                // global entity was not reserved, create a new one
                global_entity_opt = None;
            }
        } else {
            // local spawn, no remote entity
            global_entity_opt = None;
        };

        let global_entity = if let Some(global_entity) = global_entity_opt {
            global_entity
        } else {
            self.global_to_entity_map.insert(Some(world_entity))
        };

        self.entity_to_global_map
            .insert(world_entity, global_entity);

        global_entity
    }

    fn reserve_global_entity(&mut self, remote_entity: RemoteEntity) -> GlobalEntity {
        if self.reserved_global_entities.contains_key(&remote_entity) {
            panic!(
                "Remote entity {:?} already has a reserved global entity",
                remote_entity
            );
        }

        let global_entity = self.global_to_entity_map.insert(None);
        self.reserved_global_entities
            .insert(remote_entity, global_entity);

        // warn!(
        //     "Reserving global entity {:?}, for remote entity {:?}",
        //     global_entity, remote_entity
        // );

        global_entity
    }

    fn despawn_by_global(&mut self, global_entity: &GlobalEntity) {
        let Some(Some(world_entity)) = self.global_to_entity_map.remove(&global_entity) else {
            panic!(
                "Global entity {:?} does not exist in the global to entity map",
                global_entity
            );
        };
        self.entity_to_global_map.remove(&world_entity);
    }

    fn despawn_by_world(&mut self, world_entity: &E) {
        let global_entity = self.entity_to_global_map.remove(&world_entity).unwrap();
        self.global_to_entity_map.remove(&global_entity);
    }

    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }
}
