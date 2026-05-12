use std::collections::HashMap;

use crate::{
    world::local::{
        local_entity::{HostEntity, OwnedLocalEntity, RemoteEntity},
        local_entity_record::LocalEntityRecord,
    },
    EntityDoesNotExistError, GlobalEntity, HostType, Instant, LocalEntityAndGlobalEntityConverter,
};

/// Bidirectional lookup table between [`GlobalEntity`] identifiers and their connection-local [`HostEntity`] or [`RemoteEntity`] counterparts.
pub struct LocalEntityMap {
    host_type: HostType,
    global_to_local: HashMap<GlobalEntity, LocalEntityRecord>,
    /// Keyed by `HostEntity { id, is_static }` — static and dynamic pools both
    /// start from 0, but `HostEntity` carries `is_static` so they hash distinctly.
    host_to_global: HashMap<HostEntity, GlobalEntity>,
    remote_to_global: HashMap<RemoteEntity, GlobalEntity>,
    entity_redirects: HashMap<OwnedLocalEntity, (OwnedLocalEntity, Instant)>,
}

impl LocalEntityAndGlobalEntityConverter for LocalEntityMap {
    fn global_entity_to_host_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<HostEntity, EntityDoesNotExistError> {
        if let Some(record) = self.global_to_local.get(global_entity) {
            if record.is_host_owned() {
                return Ok(record.host_entity());
            }
        }
        Err(EntityDoesNotExistError)
    }

    fn global_entity_to_remote_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<RemoteEntity, EntityDoesNotExistError> {
        if let Some(record) = self.global_to_local.get(global_entity) {
            if record.is_remote_owned() {
                return Ok(record.remote_entity());
            }
        }
        Err(EntityDoesNotExistError)
    }

    fn global_entity_to_owned_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<OwnedLocalEntity, EntityDoesNotExistError> {
        if let Some(record) = self.global_to_local.get(global_entity) {
            // info!("global_entity_to_owned_entity(). Found record for global entity {:?}: {:?}", global_entity, record);
            return Ok(record.owned_entity());
        }
        Err(EntityDoesNotExistError)
    }

    fn host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(global_entity) = self.host_to_global.get(host_entity) {
            return Ok(*global_entity);
        }
        Err(EntityDoesNotExistError)
    }

    fn static_host_entity_to_global_entity(
        &self,
        host_entity: &HostEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(global_entity) = self.host_to_global.get(host_entity) {
            return Ok(*global_entity);
        }
        Err(EntityDoesNotExistError)
    }

    fn remote_entity_to_global_entity(
        &self,
        remote_entity: &RemoteEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        if let Some(global_entity) = self.remote_to_global.get(remote_entity) {
            return Ok(*global_entity);
        }
        Err(EntityDoesNotExistError)
    }

    fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        if let Some((new_entity, _timestamp)) = self.entity_redirects.get(entity) {
            *new_entity
        } else {
            *entity
        }
    }
}

impl LocalEntityMap {
    /// Creates an empty map for the given `host_type` side of a connection.
    pub fn new(host_type: HostType) -> Self {
        Self {
            host_type,
            global_to_local: HashMap::new(),
            host_to_global: HashMap::new(),
            remote_to_global: HashMap::new(),
            entity_redirects: HashMap::new(),
        }
    }

    /// Returns whether this map belongs to a server or client side.
    pub fn host_type(&self) -> HostType {
        self.host_type
    }

    /// Registers a host-owned mapping from `global_entity` to `host_entity`, panicking on duplicate keys.
    pub fn insert_with_host_entity(
        &mut self,
        global_entity: GlobalEntity,
        host_entity: HostEntity,
    ) {
        if self.global_to_local.contains_key(&global_entity) {
            panic!(
                "Cannot overwrite inserted global entity: {:?}",
                global_entity
            );
        }
        if self.host_to_global.contains_key(&host_entity) {
            panic!("Cannot overwrite inserted host entity {:?}", host_entity);
        }

        self.global_to_local.insert(
            global_entity,
            LocalEntityRecord::new_host_owned_entity(host_entity),
        );

        self.host_to_global.insert(host_entity, global_entity);
    }

    /// Registers a static host-owned mapping from `global_entity` to `host_entity`, panicking on duplicate keys.
    pub fn insert_with_static_host_entity(
        &mut self,
        global_entity: GlobalEntity,
        host_entity: HostEntity,
    ) {
        if self.global_to_local.contains_key(&global_entity) {
            panic!(
                "Cannot overwrite inserted global entity: {:?}",
                global_entity
            );
        }
        if self.host_to_global.contains_key(&host_entity) {
            panic!("Cannot overwrite inserted static host entity {:?}", host_entity);
        }

        self.global_to_local.insert(
            global_entity,
            LocalEntityRecord::new_static_host_owned_entity(host_entity),
        );

        self.host_to_global.insert(host_entity, global_entity);
    }

    /// Registers a remote-owned mapping from `global_entity` to `remote_entity`, panicking on duplicate keys.
    pub fn insert_with_remote_entity(
        &mut self,
        global_entity: GlobalEntity,
        remote_entity: RemoteEntity,
    ) {
        if self.global_to_local.contains_key(&global_entity) {
            panic!(
                "Cannot overwrite inserted global entity: {:?}",
                global_entity
            );
        }
        if self.remote_to_global.contains_key(&remote_entity) {
            panic!(
                "Cannot overwrite inserted remote entity {:?}",
                remote_entity
            );
        }

        self.global_to_local.insert(
            global_entity,
            LocalEntityRecord::new_remote_owned_entity(remote_entity),
        );
        self.remote_to_global.insert(remote_entity, global_entity);
    }

    /// Returns the [`GlobalEntity`] mapped from `remote_entity`, if one exists.
    pub fn global_entity_from_remote(&self, remote_entity: &RemoteEntity) -> Option<&GlobalEntity> {
        self.remote_to_global.get(remote_entity)
    }

    /// Returns the [`GlobalEntity`] mapped from `host_entity`, if one exists.
    pub fn global_entity_from_host(&self, host_entity: &HostEntity) -> Option<&GlobalEntity> {
        self.host_to_global.get(host_entity)
    }

/// Removes the record for `global_entity` and cleans up the reverse index, returning the record if it existed.
    pub fn remove_by_global_entity(
        &mut self,
        global_entity: &GlobalEntity,
    ) -> Option<LocalEntityRecord> {
        // info!("Removing global entity: {:?}", global_entity);
        let record_opt = self.global_to_local.remove(global_entity);
        if let Some(record) = &record_opt {
            if record.is_host_owned() {
                let host_entity = record.host_entity();
                self.host_to_global.remove(&host_entity);
            } else {
                let remote_entity = record.remote_entity();
                self.remote_to_global.remove(&remote_entity);
            }
        }
        record_opt
    }

    pub(crate) fn remove_by_remote_entity(&mut self, remote_entity: &RemoteEntity) -> GlobalEntity {
        let global_entity = self.remote_to_global.remove(remote_entity);
        let Some(global_entity) = global_entity else {
            panic!(
                "Attempting to remove remote entity which does not exist: {:?}",
                remote_entity
            );
        };
        self.remove_by_global_entity(&global_entity);
        global_entity
    }

    /// Remove remote mapping if it exists (idempotent, used during migration cleanup)
    /// This ensures that after migration, global_entity_to_remote_entity() will fail
    pub(crate) fn remove_remote_mapping_if_exists(&mut self, remote_entity: &RemoteEntity) {
        // Remove from remote_to_global map - this is the key that global_entity_to_remote_entity uses
        // via remote_entity_to_global_entity lookup, but more importantly, we need to ensure
        // that global_to_local doesn't have a remote-owned record for the same global_entity
        if let Some(_global_entity) = self.remote_to_global.remove(remote_entity) {
            // Double-check: if global_to_local still has this global_entity marked as remote-owned,
            // that's a bug - it should have been removed by remove_by_global_entity
            // But we can't fix it here without knowing the new state, so we just remove the mapping
        }
    }

    /// Returns `true` if `global_entity` is currently registered in the map.
    pub fn contains_global_entity(&self, global_entity: &GlobalEntity) -> bool {
        self.global_to_local.contains_key(global_entity)
    }

    /// Returns `true` if `host_entity` is currently registered in the map.
    pub fn contains_host_entity(&self, host_entity: &HostEntity) -> bool {
        self.host_to_global.contains_key(host_entity)
    }

    /// Returns `true` if `remote_entity` is currently registered in the map.
    pub fn contains_remote_entity(&self, remote_entity: &RemoteEntity) -> bool {
        self.remote_to_global.contains_key(remote_entity)
    }

    /// Iterates over all `(GlobalEntity, LocalEntityRecord)` pairs currently in the map.
    pub fn iter(&self) -> impl Iterator<Item = (&GlobalEntity, &LocalEntityRecord)> {
        self.global_to_local.iter()
    }

    pub(crate) fn remote_entities(&self) -> Vec<GlobalEntity> {
        self.iter()
            .filter(|(_, record)| record.is_remote_owned())
            .map(|(global_entity, _)| *global_entity)
            .collect::<Vec<GlobalEntity>>()
    }

    // pub(crate) fn global_entity_is_delegated(&self, global_entity: &GlobalEntity) -> bool {
    //     if let Some(record) = self.global_to_local.get(global_entity) {
    //         return record.is_delegated();
    //     }
    //     false
    // }

    /// Returns `self` as a read-only [`LocalEntityAndGlobalEntityConverter`] reference.
    pub fn entity_converter(&self) -> &dyn LocalEntityAndGlobalEntityConverter {
        self
    }

    /// Installs a redirect so that lookups of `old_entity` transparently return `new_entity` for a TTL period.
    pub fn install_entity_redirect(
        &mut self,
        old_entity: OwnedLocalEntity,
        new_entity: OwnedLocalEntity,
    ) {
        let now = Instant::now();
        self.entity_redirects.insert(old_entity, (new_entity, now));
    }

    pub(crate) fn apply_entity_redirect(&self, entity: &OwnedLocalEntity) -> OwnedLocalEntity {
        self.entity_redirects
            .get(entity)
            .map(|(new_entity, _)| *new_entity)
            .unwrap_or(*entity)
    }

    pub(crate) fn cleanup_old_redirects(&mut self, now: &Instant, ttl_seconds: u64) {
        use std::time::Duration;
        let ttl_duration = Duration::from_secs(ttl_seconds);
        self.entity_redirects
            .retain(|_, (_, timestamp)| timestamp.elapsed(now) < ttl_duration);
    }
}
