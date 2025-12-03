use std::collections::HashMap;

use crate::TestEntity;
use super::keys::EntityKey;

/// EntityRegistry maps logical EntityKeys to server host world entities
pub struct EntityRegistry {
    next_entity_key: u32,
    host_world_entities: HashMap<EntityKey, TestEntity>,
}

impl EntityRegistry {
    pub fn new() -> Self {
        Self {
            next_entity_key: 1,
            host_world_entities: HashMap::new(),
        }
    }

    /// Allocate a new EntityKey
    pub fn allocate_entity_key(&mut self) -> EntityKey {
        let key = EntityKey::new(self.next_entity_key);
        self.next_entity_key += 1;
        key
    }

    /// Register a server host world entity for an EntityKey
    pub fn register_host_entity(&mut self, entity_key: EntityKey, entity: TestEntity) {
        self.host_world_entities.insert(entity_key, entity);
    }

    /// Get the server host world entity for an EntityKey
    pub fn host_world(&self, entity_key: EntityKey) -> Option<TestEntity> {
        self.host_world_entities.get(&entity_key).copied()
    }

    /// Check if host entity mapping exists
    pub fn has_host_entity(&self, entity_key: EntityKey) -> bool {
        self.host_world_entities.contains_key(&entity_key)
    }
}

