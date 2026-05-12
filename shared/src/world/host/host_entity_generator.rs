use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use naia_socket_shared::Instant;

use crate::world::local::local_entity::{HostEntity, RemoteEntity};
use crate::world::local::local_entity_map::LocalEntityMap;
use crate::{GlobalEntity, KeyGenerator};

/// Issues and recycles wire-level [`HostEntity`] identifiers for a single connected user.
pub struct HostEntityGenerator {
    user_key: u64,
    generator: KeyGenerator<u16>,
    static_generator: KeyGenerator<u16>,
    reserved_host_entities: HashMap<GlobalEntity, HostEntity>,
    reserved_host_entity_ttl: Duration,
    reserved_host_entities_ttls: VecDeque<(Instant, GlobalEntity)>,
}

impl HostEntityGenerator {
    /// Creates a generator bound to `user_key` with fresh entity and static-entity ID pools.
    pub fn new(user_key: u64) -> Self {
        Self {
            user_key,
            generator: KeyGenerator::new(Duration::from_secs(60)),
            static_generator: KeyGenerator::new(Duration::from_secs(60)),
            reserved_host_entities: HashMap::new(),
            reserved_host_entity_ttl: Duration::from_secs(60),
            reserved_host_entities_ttls: VecDeque::new(),
        }
    }

    // Host entities

    /// Allocates a [`HostEntity`] for `global_entity` before it has been sent, expiring reservations that have timed out.
    pub fn host_reserve_entity(
        &mut self,
        entity_map: &mut LocalEntityMap,
        global_entity: &GlobalEntity,
    ) -> HostEntity {
        self.process_reserved_entity_timeouts();

        if self.reserved_host_entities.contains_key(global_entity) {
            panic!("Global Entity has already reserved Local Entity!");
        }
        let host_entity = self.generate_host_entity();
        entity_map.insert_with_host_entity(*global_entity, host_entity);
        self.reserved_host_entities
            .insert(*global_entity, host_entity);
        host_entity
    }

    fn process_reserved_entity_timeouts(&mut self) {
        let now = Instant::now();

        loop {
            let Some((timeout, _)) = self.reserved_host_entities_ttls.front() else {
                break;
            };
            if timeout.elapsed(&now) < self.reserved_host_entity_ttl {
                break;
            }
            let (_, global_entity) = self.reserved_host_entities_ttls.pop_front().unwrap();
            let Some(_) = self.reserved_host_entities.remove(&global_entity) else {
                panic!("Reserved Entity does not exist!");
            };
        }
    }

    /// Removes and returns the reserved [`HostEntity`] for `global_entity`, if one exists.
    pub fn host_remove_reserved_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Option<HostEntity> {
        self.reserved_host_entities.remove(global_entity)
    }

    pub(crate) fn generate_host_entity(&mut self) -> HostEntity {
        HostEntity::new(self.generator.generate())
    }

    // Static entities use a separate counter so their wire IDs (0, 1, 2 …)
    // never collide with dynamic entity wire IDs (also 0, 1, 2 …).
    // The is_static bit in the wire format keeps them distinguishable.
    pub(crate) fn generate_static_host_entity(&mut self) -> HostEntity {
        HostEntity::new_static(self.static_generator.generate())
    }

    pub(crate) fn remove_by_global_entity(
        &mut self,
        entity_map: &mut LocalEntityMap,
        global_entity: &GlobalEntity,
    ) {
        let record = entity_map
            .remove_by_global_entity(global_entity)
            .expect("Attempting to despawn entity which does not exist!");
        if record.is_host_owned() {
            let host_entity = record.host_entity();
            if host_entity.is_static() {
                self.static_generator.recycle_key(&host_entity.value());
            } else {
                self.generator.recycle_key(&host_entity.value());
            }
        }
    }

    pub(crate) fn remove_by_host_entity(
        &mut self,
        converter: &mut LocalEntityMap,
        host_entity: &HostEntity,
    ) {
        // The mapping may already have been cleared at send time by
        // `LocalWorldManager::despawn_entity` (see [entity-delegation-15]).
        // In that case, just recycle the id slot — the entity_map removal
        // already happened. Otherwise (legacy path, or non-despawn cleanup),
        // do the full remove.
        if let Some(global_entity) = converter.global_entity_from_host(host_entity).copied() {
            self.remove_by_global_entity(converter, &global_entity);
        } else {
            // Send-time cleanup already removed the mapping; just free the id.
            if host_entity.is_static() {
                self.static_generator.recycle_key(&host_entity.value());
            } else {
                self.generator.recycle_key(&host_entity.value());
            }
        }
    }

    /// Removes the entity identified by `remote_entity` from `entity_map`, recycles its host ID, and returns its [`GlobalEntity`].
    pub fn remove_by_remote_entity(
        &mut self,
        entity_map: &mut LocalEntityMap,
        remote_entity: &RemoteEntity,
    ) -> GlobalEntity {
        let global_entity = *(entity_map
            .global_entity_from_remote(remote_entity)
            .expect("Attempting to despawn entity which does not exist!"));
        let record = entity_map
            .remove_by_global_entity(&global_entity)
            .expect("Attempting to despawn entity which does not exist!");
        if record.is_host_owned() {
            let host_entity = record.host_entity();
            if host_entity.is_static() {
                self.static_generator.recycle_key(&host_entity.value());
            } else {
                self.generator.recycle_key(&host_entity.value());
            }
        }
        global_entity
    }

    // Misc

    /// Returns the user key this generator was created for.
    pub fn get_user_key(&self) -> &u64 {
        &self.user_key
    }
}
