use log::warn;
use naia_socket_shared::Instant;
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    time::Duration,
};

use crate::{
    world::entity::local_entity::LocalEntity, EntityAndLocalEntityConverter,
    EntityDoesNotExistError, KeyGenerator,
    doublemap::DoubleMap
};

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    user_key: u64,
    host_entity_generator: KeyGenerator<u16>,
    entity_map: DoubleMap<E, LocalEntity>,
    reserved_entities: HashMap<E, LocalEntity>,
    reserved_entity_ttl: Duration,
    reserved_entities_ttls: VecDeque<(Instant, E)>,
}

impl<E: Copy + Eq + Hash> LocalWorldManager<E> {
    pub fn new(user_key: u64) -> Self {
        Self {
            user_key,
            host_entity_generator: KeyGenerator::new(Duration::from_secs(60)),
            entity_map: DoubleMap::new(),
            reserved_entities: HashMap::new(),
            reserved_entity_ttl: Duration::from_secs(60),
            reserved_entities_ttls: VecDeque::new(),
        }
    }

    // Host entities

    pub(crate) fn host_reserve_entity(&mut self, world_entity: &E) -> LocalEntity {
        self.process_reserved_entity_timeouts();

        if self.reserved_entities.contains_key(world_entity) {
            panic!("World Entity has already reserved Local Entity!");
        }
        let host_entity = self.generate_host_entity();
        self.entity_map.insert(*world_entity, host_entity);
        self.reserved_entities.insert(*world_entity, host_entity);
        host_entity
    }

    fn process_reserved_entity_timeouts(&mut self) {
        loop {
            let Some((timeout, _)) = self.reserved_entities_ttls.front() else {
                break;
            };
            if timeout.elapsed() < self.reserved_entity_ttl {
                break;
            }
            let (_, world_entity) = self.reserved_entities_ttls.pop_front().unwrap();
            self.reserved_entities.remove(&world_entity);
            warn!("A Entity reserved for spawning on the Remote Connection just timed out. Check that the reserved Entity is able to replicate to the Remote Connection.");
        }
    }

    pub(crate) fn remove_reserved_host_entity(&mut self, world_entity: &E) -> Option<LocalEntity> {
        self.reserved_entities.remove(world_entity)
    }

    pub(crate) fn generate_host_entity(&mut self) -> LocalEntity {
        LocalEntity::Host(self.host_entity_generator.generate())
    }

    pub(crate) fn insert_entity(&mut self, world_entity: E, local_entity: LocalEntity) {
        if self.entity_map.contains_key(&world_entity) {
            panic!("World Entity already exists!");
        }
        if self.entity_map.contains_value(&local_entity) {
            panic!("Local Entity already exists!");
        }

        self.entity_map.insert(world_entity, local_entity);
    }

    pub(crate) fn remove_world_entity(&mut self, world_entity: &E) -> LocalEntity {
        self.entity_map.remove_by_key(world_entity).expect("Attempting to despawn entity which does not exist!")
    }

    pub(crate) fn remove_local_entity(&mut self, local_entity: &LocalEntity) -> E {
        self.entity_map.remove_by_value(local_entity).expect("Attempting to despawn entity which does not exist!")
    }

    pub(crate) fn recycle_host_entity(&mut self, local_entity: LocalEntity) {
        if !local_entity.is_host() {
            panic!("can only call this method with host entities");
        }
        self.host_entity_generator.recycle_key(&local_entity.value());
    }

    // Remote entities

    pub(crate) fn has_local_entity(&self, local_entity: &LocalEntity) -> bool {
        self.entity_map.contains_value(local_entity)
    }

    pub(crate) fn get_world_entity(&self, local_entity: &LocalEntity) -> E {
        // Why is this needed? Should uncomment?
        // if !local_entity.is_remote() {
        //     panic!("can only call this method with remote entities");
        // }

        let world_entity = self.entity_map.get_by_value(&local_entity).expect("Attempting to access remote entity which does not exist!");
        return *world_entity;
    }

    pub(crate) fn remote_entities(&self) -> Vec<E> {
        self.entity_map.iter().filter(|(_, local_entity)| {
            local_entity.is_remote()
        }).map(|(world_entity, _)| {
            *world_entity
        }).collect::<Vec<E>>()
    }

    pub fn get_user_key(&self) -> &u64 {
        &self.user_key
    }
}

impl<E: Copy + Eq + Hash> EntityAndLocalEntityConverter<E> for LocalWorldManager<E> {
    fn entity_to_local_entity(&self, world_entity: &E) -> Result<LocalEntity, EntityDoesNotExistError> {
        if let Some(local_entity) = self.entity_map.get_by_key(world_entity) {
            return Ok(*local_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn local_entity_to_entity(
        &self,
        local_entity: &LocalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.entity_map.get_by_value(local_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }
}
