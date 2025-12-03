use std::collections::HashMap;

use crate::TestEntity;
use super::keys::{ClientKey, EntityKey};

/// EntityRegistry maps logical EntityKeys to per-actor entity IDs
pub struct EntityRegistry {
    next_entity_key: u32,
    server_entities: HashMap<EntityKey, TestEntity>,
    client_entities: HashMap<(EntityKey, ClientKey), TestEntity>,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_entity_key: 1,
            server_entities: HashMap::new(),
            client_entities: HashMap::new(),
        }
    }

    /// Allocate a new EntityKey
    pub fn allocate_entity_key(&mut self) -> EntityKey {
        let key = EntityKey::new(self.next_entity_key);
        self.next_entity_key += 1;
        key
    }

    /// Register a client-side entity ID when spawning
    pub fn register_spawning_client(
        &mut self,
        entity_key: EntityKey,
        client_key: ClientKey,
        entity: TestEntity,
    ) {
        self.client_entities.insert((entity_key, client_key), entity);
    }

    /// Map a server-side entity ID to an EntityKey
    pub fn map_server_entity(&mut self, entity_key: EntityKey, entity: TestEntity) {
        self.server_entities.insert(entity_key, entity);
    }

    /// Map a client-side entity ID to an EntityKey
    pub fn map_client_entity(
        &mut self,
        entity_key: EntityKey,
        client_key: ClientKey,
        entity: TestEntity,
    ) {
        self.client_entities.insert((entity_key, client_key), entity);
    }

    /// Get the server-side entity ID for an EntityKey
    pub fn get_server_entity(&self, entity_key: EntityKey) -> Option<TestEntity> {
        self.server_entities.get(&entity_key).copied()
    }

    /// Get the client-side entity ID for an EntityKey
    pub fn get_client_entity(&self, entity_key: EntityKey, client_key: ClientKey) -> Option<TestEntity> {
        self.client_entities.get(&(entity_key, client_key)).copied()
    }

    /// Check if server entity mapping exists
    pub fn has_server_entity(&self, entity_key: EntityKey) -> bool {
        self.server_entities.contains_key(&entity_key)
    }

    /// Check if client entity mapping exists
    pub fn has_client_entity(&self, entity_key: EntityKey, client_key: ClientKey) -> bool {
        self.client_entities.contains_key(&(entity_key, client_key))
    }

    /// Check if a server entity is already mapped to any key
    pub fn is_server_entity_mapped(&self, entity: TestEntity) -> bool {
        self.server_entities.values().any(|&e| e == entity)
    }

    /// Check if a client entity is already mapped to a different key for this client
    pub fn is_client_entity_mapped_to_different_key(
        &self,
        entity: TestEntity,
        client_key: ClientKey,
        exclude_entity_key: EntityKey,
    ) -> bool {
        self.client_entities
            .iter()
            .any(|((ek, ck), &e)| *ck == client_key && ek != &exclude_entity_key && e == entity)
    }
}

