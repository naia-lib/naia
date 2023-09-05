use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    time::Duration,
};

use naia_socket_shared::Instant;

use crate::{
    world::{
        entity::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity},
        local_entity_map::LocalEntityMap,
    },
    EntityAndLocalEntityConverter, EntityDoesNotExistError, KeyGenerator,
};

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    user_key: u64,
    host_entity_generator: KeyGenerator<u16>,
    entity_map: LocalEntityMap<E>,
    reserved_entities: HashMap<E, HostEntity>,
    reserved_entity_ttl: Duration,
    reserved_entities_ttls: VecDeque<(Instant, E)>,
}

impl<E: Copy + Eq + Hash> LocalWorldManager<E> {
    pub fn new(user_key: u64) -> Self {
        Self {
            user_key,
            host_entity_generator: KeyGenerator::new(Duration::from_secs(60)),
            entity_map: LocalEntityMap::new(),
            reserved_entities: HashMap::new(),
            reserved_entity_ttl: Duration::from_secs(60),
            reserved_entities_ttls: VecDeque::new(),
        }
    }

    // Host entities

    pub fn host_reserve_entity(&mut self, world_entity: &E) -> HostEntity {
        self.process_reserved_entity_timeouts();

        if self.reserved_entities.contains_key(world_entity) {
            panic!("World Entity has already reserved Local Entity!");
        }
        let host_entity = self.generate_host_entity();
        self.entity_map
            .insert_with_host_entity(*world_entity, host_entity);
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
            let Some(_) = self.reserved_entities.remove(&world_entity) else {
                panic!("Reserved Entity does not exist!");
            };
        }
    }

    pub fn remove_reserved_host_entity(&mut self, world_entity: &E) -> Option<HostEntity> {
        self.reserved_entities.remove(world_entity)
    }

    pub(crate) fn generate_host_entity(&mut self) -> HostEntity {
        HostEntity::new(self.host_entity_generator.generate())
    }

    pub(crate) fn insert_host_entity(&mut self, world_entity: E, host_entity: HostEntity) {
        if self.entity_map.contains_host_entity(&host_entity) {
            panic!("Local Entity already exists!");
        }

        self.entity_map
            .insert_with_host_entity(world_entity, host_entity);
    }

    pub fn insert_remote_entity(&mut self, world_entity: &E, remote_entity: RemoteEntity) {
        if self.entity_map.contains_remote_entity(&remote_entity) {
            panic!("Remote Entity already exists!");
        }

        self.entity_map
            .insert_with_remote_entity(*world_entity, remote_entity);
    }

    pub(crate) fn remove_by_world_entity(&mut self, world_entity: &E) {
        let record = self.entity_map
            .remove_by_world_entity(world_entity)
            .expect("Attempting to despawn entity which does not exist!");
        let host_entity = record.host().unwrap();
        self.recycle_host_entity(host_entity);
    }

    pub fn remove_by_remote_entity(&mut self, remote_entity: &RemoteEntity) -> E {
        let world_entity = *(self
            .entity_map
            .world_entity_from_remote(remote_entity)
            .expect("Attempting to despawn entity which does not exist!"));
        let record = self.entity_map
            .remove_by_world_entity(&world_entity)
            .expect("Attempting to despawn entity which does not exist!");
        if let Some(host_entity) = record.host() {
            self.recycle_host_entity(host_entity);
        }
        world_entity
    }

    pub(crate) fn recycle_host_entity(&mut self, host_entity: HostEntity) {
        self.host_entity_generator.recycle_key(&host_entity.value());
    }

    pub(crate) fn has_host_entity(&self, host_entity: &HostEntity) -> bool {
        self.entity_map.contains_host_entity(host_entity)
    }

    // Remote entities

    pub fn has_remote_entity(&self, remote_entity: &RemoteEntity) -> bool {
        self.entity_map.contains_remote_entity(remote_entity)
    }

    pub(crate) fn world_entity_from_remote(&self, remote_entity: &RemoteEntity) -> E {
        if let Some(world_entity) = self.entity_map.world_entity_from_remote(remote_entity) {
            return *world_entity;
        } else {
            panic!(
                "Attempting to get world entity for local entity which does not exist!: `{:?}`",
                remote_entity
            );
        }
    }

    pub(crate) fn remote_entities(&self) -> Vec<E> {
        self.entity_map
            .iter()
            .filter(|(_, record)| record.is_only_remote())
            .map(|(world_entity, _)| *world_entity)
            .collect::<Vec<E>>()
    }

    // Misc

    pub fn has_both_host_and_remote_entity(&self, world_entity: &E) -> bool {
        self.entity_map
            .has_both_host_and_remote_entity(world_entity)
    }

    pub fn has_world_entity(&self, world_entity: &E) -> bool {
        self.entity_map.contains_world_entity(world_entity)
    }

    pub fn remove_redundant_host_entity(&mut self, world_entity: &E) {
        let host_entity = self.entity_map.remove_redundant_host_entity(world_entity);
        self.recycle_host_entity(host_entity);
    }

    pub fn remove_redundant_remote_entity(&mut self, world_entity: &E) -> RemoteEntity {
        self.entity_map.remove_redundant_remote_entity(world_entity)
    }

    pub fn get_user_key(&self) -> &u64 {
        &self.user_key
    }
}

impl<E: Copy + Eq + Hash> EntityAndLocalEntityConverter<E> for LocalWorldManager<E> {
    fn entity_to_host_entity(
        &self,
        world_entity: &E,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        if let Some(local_entity) = self.entity_map.get_host_entity(world_entity) {
            return Ok(local_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn entity_to_remote_entity(
        &self,
        world_entity: &E,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        if let Some(local_entity) = self.entity_map.get_remote_entity(world_entity) {
            return Ok(local_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn entity_to_owned_entity(
        &self,
        world_entity: &E,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if let Some(local_entity) = self.entity_map.get_owned_entity(world_entity) {
            return Ok(local_entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn host_entity_to_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.entity_map.world_entity_from_host(host_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }

    fn remote_entity_to_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        if let Some(entity) = self.entity_map.world_entity_from_remote(remote_entity) {
            return Ok(*entity);
        } else {
            return Err(EntityDoesNotExistError);
        }
    }
}
