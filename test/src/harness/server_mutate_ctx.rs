use naia_server::ReplicationConfig as ServerReplicationConfig;
use naia_shared::WorldRefType;

use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Context for server-side actions
pub struct ServerMutateCtx<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> ServerMutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    /// Include an entity in scope for a client
    pub fn include_in_scope(&mut self, client: ClientKey, entity: EntityKey) {
        // Auto-discover server entity if not mapped yet
        if !self.scenario.entity_registry().has_server_entity(entity) {
            self.auto_discover_server_entity(entity);
        }

        let server_entity = self
            .scenario
            .entity_registry()
            .get_server_entity(entity)
            .expect("EntityKey not mapped to server entity after auto-discovery");

        let user_key = self.scenario.user_key(client);
        
        // Get main room key (copy it since RoomKey is Copy)
        let main_room = *self.scenario.main_room_key().expect("main room should exist");
        
        // Entity must be in a room for scope to work
        // Also, client-spawned entities need to be Public to be visible to other clients
        // Add entity to main room if not already there, configure as Public, then add to user's scope
        
        // Step 1: Add entity to room
        {
            let server = self.scenario.server_mut();
            if !server.room_mut(&main_room).has_entity(&server_entity) {
                server.room_mut(&main_room).add_entity(&server_entity);
            }
        }
        
        // Step 2: Ensure entity is Public on the server side
        // Even though we configured it as Public on the client, we need to ensure
        // the server has processed the publish message and the entity is actually Public
        // before including it in scope for other clients
        {
            let server = self.scenario.server_mut();
            let config = server.entity_replication_config(&server_entity);
            if config != Some(ServerReplicationConfig::Public) {
                // Entity isn't Public yet on server - configure it here
                // This can happen if the client's publish message hasn't been processed yet
                self.scenario.configure_entity_replication(
                    &server_entity,
                    ServerReplicationConfig::Public,
                );
            }
        }
        
        // Add entity to user's scope
        self.scenario
            .server_mut()
            .user_scope_mut(&user_key)
            .include(&server_entity);
    }

    /// Auto-discover server entity using simple heuristic (first entity)
    fn auto_discover_server_entity(&mut self, entity_key: EntityKey) {
        let entities = self.scenario.server_world_mut().proxy().entities();
        if !entities.is_empty() {
            // Use first entity that isn't already mapped to another key
            let mut candidate = None;
            for entity in &entities {
                // Check if this entity is already mapped to a different key
                if !self.scenario.entity_registry().is_server_entity_mapped(*entity) {
                    candidate = Some(*entity);
                    break;
                }
            }
            // If all entities are mapped, just use the first one (fallback)
            let entity_to_map = candidate.unwrap_or(entities[0]);
            self.scenario
                .entity_registry_mut()
                .map_server_entity(entity_key, entity_to_map);
        } else {
            panic!("Auto-discovery failed: no entities found on server world for EntityKey {:?}", entity_key);
        }
    }
}