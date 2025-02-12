use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use naia_socket_shared::Instant;

use crate::{world::{
    entity::local_entity::{HostEntity, RemoteEntity},
    local_entity_map::LocalEntityMap,
}, GlobalEntity, KeyGenerator, LocalEntityAndGlobalEntityConverter};

pub struct LocalWorldManager {
    user_key: u64,
    host_entity_generator: KeyGenerator<u16>,
    entity_map: LocalEntityMap,
    reserved_entities: HashMap<GlobalEntity, HostEntity>,
    reserved_entity_ttl: Duration,
    reserved_entities_ttls: VecDeque<(Instant, GlobalEntity)>,
}

impl LocalWorldManager {
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

    pub fn entity_converter(&self) -> &dyn LocalEntityAndGlobalEntityConverter {
        &self.entity_map
    }

    // Host entities

    pub fn host_reserve_entity(&mut self, world_entity: &GlobalEntity) -> HostEntity {
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
        let now = Instant::now();

        loop {
            let Some((timeout, _)) = self.reserved_entities_ttls.front() else {
                break;
            };
            if timeout.elapsed(&now) < self.reserved_entity_ttl {
                break;
            }
            let (_, world_entity) = self.reserved_entities_ttls.pop_front().unwrap();
            let Some(_) = self.reserved_entities.remove(&world_entity) else {
                panic!("Reserved Entity does not exist!");
            };
        }
    }

    pub fn remove_reserved_host_entity(&mut self, global_entity: &GlobalEntity) -> Option<HostEntity> {
        self.reserved_entities.remove(global_entity)
    }

    pub(crate) fn generate_host_entity(&mut self) -> HostEntity {
        HostEntity::new(self.host_entity_generator.generate())
    }

    pub(crate) fn insert_host_entity(&mut self, world_entity: GlobalEntity, host_entity: HostEntity) {
        if self.entity_map.contains_host_entity(&host_entity) {
            panic!("Local Entity already exists!");
        }

        self.entity_map
            .insert_with_host_entity(world_entity, host_entity);
    }

    pub fn insert_remote_entity(&mut self, global_entity: &GlobalEntity, remote_entity: RemoteEntity) {
        if self.entity_map.contains_remote_entity(&remote_entity) {
            panic!("Remote Entity `{:?}` already exists!", remote_entity);
        }

        self.entity_map.insert_with_remote_entity(*global_entity, remote_entity);
    }

    pub(crate) fn remove_by_world_entity(&mut self, world_entity: &GlobalEntity) {
        let record = self
            .entity_map
            .remove_by_world_entity(world_entity)
            .expect("Attempting to despawn entity which does not exist!");
        let host_entity = record.host().unwrap();
        self.recycle_host_entity(host_entity);
    }

    pub fn remove_by_remote_entity(&mut self, remote_entity: &RemoteEntity) -> GlobalEntity {
        let world_entity = *(self
            .entity_map
            .world_entity_from_remote(remote_entity)
            .expect("Attempting to despawn entity which does not exist!"));
        let record = self
            .entity_map
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

    // Remote entities

    pub fn has_remote_entity(&self, remote_entity: &RemoteEntity) -> bool {
        self.entity_map.contains_remote_entity(remote_entity)
    }

    pub(crate) fn global_entity_from_remote(&self, remote_entity: &RemoteEntity) -> GlobalEntity {
        if let Some(world_entity) = self.entity_map.world_entity_from_remote(remote_entity) {
            return *world_entity;
        } else {
            panic!(
                "Attempting to get world entity for local entity which does not exist!: `{:?}`",
                remote_entity
            );
        }
    }

    pub(crate) fn remote_entities(&self) -> Vec<GlobalEntity> {
        self.entity_map
            .iter()
            .filter(|(_, record)| record.is_only_remote())
            .map(|(world_entity, _)| *world_entity)
            .collect::<Vec<GlobalEntity>>()
    }

    // Misc

    pub fn has_both_host_and_remote_entity(&self, world_entity: &GlobalEntity) -> bool {
        self.entity_map
            .has_both_host_and_remote_entity(world_entity)
    }

    pub fn has_world_entity(&self, world_entity: &GlobalEntity) -> bool {
        self.entity_map.contains_world_entity(world_entity)
    }

    pub fn remove_redundant_host_entity(&mut self, world_entity: &GlobalEntity) {
        if let Some(host_entity) = self.entity_map.remove_redundant_host_entity(world_entity) {
            self.recycle_host_entity(host_entity);
        }
    }

    pub fn remove_redundant_remote_entity(&mut self, world_entity: &GlobalEntity) -> RemoteEntity {
        self.entity_map.remove_redundant_remote_entity(world_entity)
    }

    pub fn get_user_key(&self) -> &u64 {
        &self.user_key
    }
}