use std::collections::HashMap;

use log::warn;

use crate::{world::entity::local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity}, EntityDoesNotExistError, GlobalEntity, LocalEntityAndGlobalEntityConverter};

#[derive(Debug)]
pub struct LocalEntityRecord {
    host: Option<HostEntity>,
    remote: Option<RemoteEntity>,
}

impl LocalEntityRecord {
    pub fn new_with_host(host: HostEntity) -> Self {
        Self {
            host: Some(host),
            remote: None,
        }
    }

    pub fn new_with_remote(remote: RemoteEntity) -> Self {
        Self {
            host: None,
            remote: Some(remote),
        }
    }

    pub(crate) fn host(&self) -> Option<HostEntity> {
        self.host
    }

    pub(crate) fn is_only_remote(&self) -> bool {
        self.host.is_none() && self.remote.is_some()
    }
}

pub struct LocalEntityMap {
    world_to_local: HashMap<GlobalEntity, LocalEntityRecord>,
    host_to_world: HashMap<HostEntity, GlobalEntity>,
    remote_to_world: HashMap<RemoteEntity, GlobalEntity>,
}

impl LocalEntityAndGlobalEntityConverter for LocalEntityMap {
    fn global_entity_to_host_entity(&self, global_entity: &GlobalEntity) -> Result<HostEntity, EntityDoesNotExistError> {
        if let Some(record) = self.world_to_local.get(global_entity) {
            if let Some(host) = record.host {
                return Ok(host);
            }
        }
        Err(EntityDoesNotExistError)
    }

    fn global_entity_to_remote_entity(&self, global_entity: &GlobalEntity) -> Result<RemoteEntity, EntityDoesNotExistError> {
        if let Some(record) = self.world_to_local.get(global_entity) {
            if let Some(remote) = record.remote {
                return Ok(remote);
            }
        }
        Err(EntityDoesNotExistError)
    }

    fn global_entity_to_owned_entity(&self, global_entity: &GlobalEntity) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if let Some(record) = self.world_to_local.get(global_entity) {
            if let Some(remote) = record.remote {
                return Ok(OwnedLocalEntity::Remote(remote.value()));
            } else if let Some(host) = record.host {
                return Ok(OwnedLocalEntity::Host(host.value()));
            }
        }
        Err(EntityDoesNotExistError)
    }

    fn host_entity_to_global_entity(&self, host_entity: &HostEntity) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(world_entity) = self.host_to_world.get(host_entity) {
            return Ok(*world_entity);
        }
        Err(EntityDoesNotExistError)
    }

    fn remote_entity_to_global_entity(&self, remote_entity: &RemoteEntity) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(world_entity) = self.remote_to_world.get(remote_entity) {
            return Ok(*world_entity);
        }
        Err(EntityDoesNotExistError)
    }
}

impl LocalEntityMap {
    pub fn new() -> Self {
        Self {
            world_to_local: HashMap::new(),
            host_to_world: HashMap::new(),
            remote_to_world: HashMap::new(),
        }
    }

    pub fn insert_with_host_entity(&mut self, world_entity: GlobalEntity, host: HostEntity) {
        if let Some(record) = self.world_to_local.get_mut(&world_entity) {
            record.host = Some(host);
        } else {
            self.world_to_local
                .insert(world_entity, LocalEntityRecord::new_with_host(host));
        }
        self.host_to_world.insert(host, world_entity);
    }

    pub fn insert_with_remote_entity(&mut self, world_entity: GlobalEntity, remote: RemoteEntity) {
        if let Some(record) = self.world_to_local.get_mut(&world_entity) {
            record.remote = Some(remote);
        } else {
            self.world_to_local
                .insert(world_entity, LocalEntityRecord::new_with_remote(remote));
        }
        self.remote_to_world.insert(remote, world_entity);
    }

    pub fn world_entity_from_remote(&self, remote_entity: &RemoteEntity) -> Option<&GlobalEntity> {
        self.remote_to_world.get(remote_entity)
    }

    pub fn remove_by_world_entity(&mut self, world: &GlobalEntity) -> Option<LocalEntityRecord> {
        let record_opt = self.world_to_local.remove(world);
        if let Some(record) = &record_opt {
            if let Some(host) = record.host {
                self.host_to_world.remove(&host);
            }
            if let Some(remote) = record.remote {
                self.remote_to_world.remove(&remote);
            }
        }
        record_opt
    }

    pub fn remove_redundant_host_entity(&mut self, world_entity: &GlobalEntity) -> Option<HostEntity> {
        if let Some(record) = self.world_to_local.get_mut(world_entity) {
            if record.host.is_some() && record.remote.is_some() {
                let Some(host_entity) = record.host.take() else {
                    panic!("record does not have host entity");
                };
                self.host_to_world.remove(&host_entity);
                return Some(host_entity);
            } else {
                panic!("record does not have dual host and remote entity");
            }
        } else {
            warn!("remove_redundant_host_entity: no record exists for entity .. removed some other way?");
            return None;
        }
    }

    pub fn remove_redundant_remote_entity(&mut self, world_entity: &GlobalEntity) -> RemoteEntity {
        let Some(record) = self.world_to_local.get_mut(world_entity) else {
            panic!("no record exists for entity");
        };
        if record.host.is_some() && record.remote.is_some() {
            let Some(remote_entity) = record.remote.take() else {
                panic!("record does not have remote entity");
            };
            self.remote_to_world.remove(&remote_entity);
            return remote_entity;
        } else {
            panic!("record does not have dual host and remote entity");
        }
    }

    pub fn has_both_host_and_remote_entity(&self, world_entity: &GlobalEntity) -> bool {
        if let Some(record) = self.world_to_local.get(world_entity) {
            if record.host.is_some() && record.remote.is_some() {
                return true;
            }
        }
        return false;
    }

    pub fn contains_world_entity(&self, world: &GlobalEntity) -> bool {
        self.world_to_local.contains_key(world)
    }

    pub fn contains_host_entity(&self, host_entity: &HostEntity) -> bool {
        self.host_to_world.contains_key(host_entity)
    }

    pub fn contains_remote_entity(&self, remote_entity: &RemoteEntity) -> bool {
        self.remote_to_world.contains_key(remote_entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&GlobalEntity, &LocalEntityRecord)> {
        self.world_to_local.iter()
    }
}
