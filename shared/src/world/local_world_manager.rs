use log::warn;
use std::{collections::{HashMap, VecDeque}, hash::Hash, time::Duration};
use naia_socket_shared::Instant;

use crate::{
    world::entity::local_entity::LocalEntity, EntityDoesNotExistError, KeyGenerator,
    LocalEntityConverter,
};

pub struct LocalWorldManager<E: Copy + Eq + Hash> {
    user_key: u64,
    host_entity_generator: KeyGenerator<u16>,
    world_to_local_entity: HashMap<E, LocalEntity>,
    local_to_world_entity: HashMap<LocalEntity, E>,
    reserved_entities: HashMap<E, LocalEntity>,
    reserved_entity_ttl: Duration,
    reserved_entities_ttls: VecDeque<(Instant, E)>,
}

impl<E: Copy + Eq + Hash> LocalWorldManager<E> {
    pub fn new(user_key: u64) -> Self {
        Self {
            user_key,
            host_entity_generator: KeyGenerator::new(Duration::from_secs(60)),
            world_to_local_entity: HashMap::new(),
            local_to_world_entity: HashMap::new(),
            reserved_entities: HashMap::new(),
            reserved_entity_ttl: Duration::from_secs(60),
            reserved_entities_ttls: VecDeque::new(),
        }
    }

    // Host entities

    pub(crate) fn host_reserve_entity(&mut self, world_entity: &E) -> LocalEntity {

        self.process_reserved_entity_timeouts();

        warn!("Reserving LocalEntity because World Entity is not yet spawned.");
        warn!("Make sure to put a TTL on this LocalEntity in the future!");

        if self.reserved_entities.contains_key(world_entity) {
            panic!("World Entity has already reserved Local Entity!");
        }
        let host_entity = self.host_spawn_entity(world_entity);
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

    pub(crate) fn host_spawn_entity(&mut self, world_entity: &E) -> LocalEntity {
        if let Some(local_entity) = self.reserved_entities.remove(world_entity) {
            return local_entity;
        }

        let host_entity = LocalEntity::Host(self.host_entity_generator.generate());

        if self.world_to_local_entity.contains_key(world_entity) {
            panic!("Entity already exists!");
        }
        if self.local_to_world_entity.contains_key(&host_entity) {
            panic!("Host Entity already exists!");
        }

        self.world_to_local_entity
            .insert(*world_entity, host_entity);
        self.local_to_world_entity
            .insert(host_entity, *world_entity);

        host_entity
    }

    pub(crate) fn host_despawn_entity(&mut self, world_entity: &E) {
        let local_entity = self
            .world_to_local_entity
            .remove(world_entity)
            .expect("Entity does not exist!");
        if !self.local_to_world_entity.contains_key(&local_entity) {
            panic!("Local Entity does not exist!");
        }
        self.local_to_world_entity.remove(&local_entity);
        self.host_entity_generator
            .recycle_key(&local_entity.value());
    }

    // Remote entities

    pub(crate) fn get_world_entity(&self, local_entity: &LocalEntity) -> E {
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

    pub fn get_user_key(&self) -> &u64 {
        &self.user_key
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
