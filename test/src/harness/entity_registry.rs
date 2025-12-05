use std::collections::HashMap;

use crate::TestEntity;
use super::keys::{EntityKey, ClientKey};
use naia_shared::LocalEntity;

/// Record tracking all entity mappings for a logical EntityKey
/// Each EntityKey MUST have at least one Some(TestEntity) - either server_entity or at least one client_entity
pub struct EntityKeyRecord {
    /// Server host world entity (None for client-spawned entities until they replicate to server)
    pub server_entity: Option<TestEntity>,
    /// Client world entities - each client gets their own TestEntity for this logical entity
    /// The LocalEntity is the same for Server<->Client pairs, but different between clients
    pub client_entities: HashMap<ClientKey, Option<TestEntity>>,
}

impl EntityKeyRecord {
    pub fn new() -> Self {
        Self {
            server_entity: None,
            client_entities: HashMap::new(),
        }
    }

    /// Check if at least one entity mapping exists (invariant: must always be true)
    pub fn has_any_entity(&self) -> bool {
        self.server_entity.is_some() || self.client_entities.values().any(|e| e.is_some())
    }
}

/// EntityRegistry is the source of truth for all EntityKey mappings
/// Maps logical EntityKeys to their server and client TestEntity instances
pub struct EntityRegistry {
    next_entity_key: u32,
    entity_map: HashMap<EntityKey, EntityKeyRecord>,
    /// Reverse mapping: (ClientKey, LocalEntity) -> EntityKey
    /// Used to match entities when clients receive SpawnEntityEvent
    /// LocalEntity is the same for Server<->Client pairs (same user), different between clients
    client_entity_to_entity_key: HashMap<(ClientKey, LocalEntity), EntityKey>,
    /// Reverse mapping: server TestEntity -> EntityKey
    /// Used to quickly find EntityKey when processing server SpawnEntityEvent
    server_entity_to_entity_key: HashMap<TestEntity, EntityKey>,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_entity_key: 1,
            entity_map: HashMap::new(),
            client_entity_to_entity_key: HashMap::new(),
            server_entity_to_entity_key: HashMap::new(),
        }
    }

    /// Allocate a new EntityKey and create its record
    pub fn allocate_entity_key(&mut self) -> EntityKey {
        let key = EntityKey::new(self.next_entity_key);
        self.next_entity_key += 1;
        self.entity_map.insert(key, EntityKeyRecord::new());
        key
    }

    /// Get or create the record for an EntityKey
    pub(crate) fn get_or_create_record(&mut self, entity_key: EntityKey) -> &mut EntityKeyRecord {
        self.entity_map.entry(entity_key).or_insert_with(EntityKeyRecord::new)
    }
    
    /// Register just the LocalEntity mapping (for server-spawned entities where client entity isn't available yet)
    /// The client entity will be registered later when the client receives SpawnEntityEvent
    pub fn register_client_local_entity_mapping(&mut self, entity_key: EntityKey, client_key: ClientKey, local_entity: LocalEntity) {
        self.client_entity_to_entity_key.insert((client_key, local_entity), entity_key);
    }

    /// Register a server host world entity for an EntityKey
    pub fn register_server_entity(&mut self, entity_key: EntityKey, entity: TestEntity) {
        let record = self.get_or_create_record(entity_key);
        record.server_entity = Some(entity);
        // Also register reverse mapping for fast lookup
        self.server_entity_to_entity_key.insert(entity, entity_key);
    }

    /// Register a client's TestEntity and LocalEntity mapping for an EntityKey
    /// This stores both the TestEntity and the reverse LocalEntity -> EntityKey mapping
    pub fn register_client_entity(&mut self, entity_key: EntityKey, client_key: ClientKey, entity: TestEntity, local_entity: LocalEntity) {
        let record = self.get_or_create_record(entity_key);
        record.client_entities.insert(client_key, Some(entity));
        // Also register reverse mapping for fast lookup
        self.client_entity_to_entity_key.insert((client_key, local_entity), entity_key);
    }


    /// Get the server host world entity for an EntityKey
    pub fn server_entity(&self, entity_key: EntityKey) -> Option<TestEntity> {
        self.entity_map.get(&entity_key)?.server_entity
    }

    /// Get a client's TestEntity for an EntityKey
    pub fn client_entity(&self, entity_key: EntityKey, client_key: ClientKey) -> Option<TestEntity> {
        self.entity_map.get(&entity_key)?
            .client_entities.get(&client_key)
            .and_then(|opt| opt.as_ref().copied())
    }


    /// Check if server entity mapping exists
    pub fn has_server_entity(&self, entity_key: EntityKey) -> bool {
        self.entity_map.get(&entity_key).map(|r| r.server_entity.is_some()).unwrap_or(false)
    }

    /// Check if client entity mapping exists
    pub fn has_client_entity(&self, entity_key: EntityKey, client_key: ClientKey) -> bool {
        self.entity_map.get(&entity_key)
            .and_then(|r| r.client_entities.get(&client_key))
            .map(|e| e.is_some())
            .unwrap_or(false)
    }


    /// Look up EntityKey from a client's LocalEntity
    /// Returns None if the mapping doesn't exist yet (entity hasn't been replicated to that client)
    pub fn entity_key_for_client_entity(&self, client_key: ClientKey, local_entity: LocalEntity) -> Option<EntityKey> {
        self.client_entity_to_entity_key.get(&(client_key, local_entity)).copied()
    }
    
    /// Look up EntityKey from a server TestEntity
    /// Returns None if the server entity isn't registered yet
    pub fn entity_key_for_server_entity(&self, server_entity: TestEntity) -> Option<EntityKey> {
        self.server_entity_to_entity_key.get(&server_entity).copied()
    }
}

