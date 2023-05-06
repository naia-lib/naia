// A Hashmap that can be queried by either key or value.

use std::{collections::HashMap, hash::Hash};

use crate::LocalEntity;

pub struct LocalEntityRecord {
    host: Option<LocalEntity>,
    remote: Option<LocalEntity>,
}

impl LocalEntityRecord {
    pub fn new_with_host(host: LocalEntity) -> Self {
        Self {
            host: Some(host),
            remote: None,
        }
    }

    pub fn new_with_remote(remote: LocalEntity) -> Self {
        Self {
            host: None,
            remote: Some(remote),
        }
    }

    pub(crate) fn host(&self) -> Option<LocalEntity> {
        self.host
    }

    pub(crate) fn remote(&self) -> Option<LocalEntity> {
        self.remote
    }

    pub(crate) fn is_only_remote(&self) -> bool {
        self.host.is_none() && self.remote.is_some()
    }
}

pub struct LocalEntityMap<E: Copy + Eq + Hash> {
    world_to_local: HashMap<E, LocalEntityRecord>,
    local_to_world: HashMap<LocalEntity, E>,
}

impl<E: Copy + Eq + Hash> LocalEntityMap<E> {
    pub fn new() -> Self {
        Self {
            world_to_local: HashMap::new(),
            local_to_world: HashMap::new(),
        }
    }

    pub fn insert_with_host_entity(&mut self, world: E, host: LocalEntity) {
        if host.is_remote() {
            panic!("Cannot insert a remote LocalEntity!");
        }
        self.world_to_local.insert(world, LocalEntityRecord::new_with_host(host));
        self.local_to_world.insert(host, world);
    }

    pub fn insert_with_remote_entity(&mut self, world: E, remote: LocalEntity) {
        if !remote.is_remote() {
            panic!("Cannot insert a host LocalEntity!");
        }
        self.world_to_local.insert(world, LocalEntityRecord::new_with_remote(remote));
        self.local_to_world.insert(remote, world);
    }

    pub fn get_host_entity(&self, world: &E) -> Option<LocalEntity> {
        self.world_to_local.get(world).map(|record| record.host).flatten()
    }

    pub fn get_remote_entity(&self, world: &E) -> Option<LocalEntity> {
        self.world_to_local.get(world).map(|record| record.remote).flatten()
    }

    pub fn get_world_entity(&self, local: &LocalEntity) -> Option<&E> {
        self.local_to_world.get(local)
    }

    pub fn remove_by_world_entity(&mut self, world: &E) -> Option<LocalEntityRecord> {
        let record_opt = self.world_to_local.remove(world);
        if let Some(record) = &record_opt {
            if let Some(host) = record.host {
                self.local_to_world.remove(&host);
            }
            if let Some(remote) = record.remote {
                self.local_to_world.remove(&remote);
            }
        }
        record_opt
    }

    pub fn contains_world_entity(&self, world: &E) -> bool {
        self.world_to_local.contains_key(world)
    }

    pub fn contains_local_entity(&self, local: &LocalEntity) -> bool {
        self.local_to_world.contains_key(local)
    }

    pub fn len(&self) -> usize {
        self.world_to_local.len()
    }

    pub fn is_empty(&self) -> bool {
        self.world_to_local.is_empty()
    }

    pub fn clear(&mut self) {
        self.world_to_local.clear();
        self.local_to_world.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&E, &LocalEntityRecord)> {
        self.world_to_local.iter()
    }
}
