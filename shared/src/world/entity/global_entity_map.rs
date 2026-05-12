use std::{collections::HashMap, hash::Hash};

use crate::{
    BigMap, EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity, RemoteEntity,
};

/// Bidirectional map between world-local entities and their stable [`GlobalEntity`] identifiers.
pub struct GlobalEntityMap<E: Copy + Eq + Hash + Send + Sync> {
    entity_to_global_map: HashMap<E, GlobalEntity>,
    global_to_entity_map: BigMap<GlobalEntity, Option<E>>,
    reserved_global_entities: HashMap<RemoteEntity, GlobalEntity>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Default for GlobalEntityMap<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> GlobalEntityMap<E> {
    /// Creates an empty map with no entity registrations.
    pub fn new() -> Self {
        Self {
            entity_to_global_map: HashMap::new(),
            global_to_entity_map: BigMap::new(),
            reserved_global_entities: HashMap::new(),
        }
    }

    /// Returns the number of world entities currently registered in the map.
    pub fn entity_count(&self) -> usize {
        self.entity_to_global_map.len()
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        self.global_to_entity_map.set_current_index_for_test(value);
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
                Err(EntityDoesNotExistError)
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

/// Extends [`EntityAndGlobalEntityConverter`] with the ability to create, reserve, and destroy entity mappings.
pub trait GlobalEntitySpawner<E: Copy + Eq + Hash + Send + Sync>:
    EntityAndGlobalEntityConverter<E>
{
    /// Registers `world_entity` in the map, reusing a reserved [`GlobalEntity`] for `remote_entity_opt` if one exists.
    fn spawn(&mut self, world_entity: E, remote_entity_opt: Option<RemoteEntity>) -> GlobalEntity;
    /// Pre-allocates a [`GlobalEntity`] slot for `remote_entity` before the local world entity is spawned.
    fn reserve_global_entity(&mut self, remote_entity: RemoteEntity) -> GlobalEntity;
    /// Removes the mapping keyed by `global_entity`, panicking if it does not exist.
    fn despawn_by_global(&mut self, global_entity: &GlobalEntity);
    /// Removes the mapping keyed by `world_entity`, panicking if it does not exist.
    fn despawn_by_world(&mut self, world_entity: &E);
    /// Returns `self` as an [`EntityAndGlobalEntityConverter`] reference for read-only lookups.
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
        let Some(Some(world_entity)) = self.global_to_entity_map.remove(global_entity) else {
            panic!(
                "Global entity {:?} does not exist in the global to entity map",
                global_entity
            );
        };
        self.entity_to_global_map.remove(&world_entity);
    }

    fn despawn_by_world(&mut self, world_entity: &E) {
        let global_entity = self.entity_to_global_map.remove(world_entity).unwrap();
        self.global_to_entity_map.remove(&global_entity);
    }

    fn to_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        self
    }
}
