use std::net::SocketAddr;

use naia_shared::EntityAuthStatus;
use naia_demo_world::WorldRef;
use naia_client::{ReplicationConfig, EntityRef, ConnectionStatus, NaiaClientError, EntityOwner};


use crate::harness::ExpectCtx;
use crate::TestEntity;
use super::keys::{ClientKey, EntityKey};

/// Context for client-side expectations
pub struct ClientExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
    pub(crate) client_key: ClientKey,
}

impl<'b, 'a: 'b> ClientExpectCtx<'b, 'a> {
    /// Expect that this client will eventually see the logical entity
    pub fn sees(&mut self, entity: &EntityKey) -> bool {
        let user_key = self.expect_ctx.scenario.user_key(&self.client_key);
        if let Some(local_entity) = self.expect_ctx.scenario.local_entity_for(entity, &user_key) {
            let state = self.expect_ctx.scenario.client_state_mut(&self.client_key);
            let local_entities = state.client.local_entities();
            local_entities.contains(&local_entity)
        } else {
            // Return false so the expect loop can keep ticking until the entity is replicated
            false
        }
    }

    /// Get read-only entity access by EntityKey
    /// Returns None if the entity doesn't exist or isn't visible to this client
    pub fn entity(&'_ mut self, key: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let user_key = self.expect_ctx.scenario.user_key(&self.client_key);
        self.expect_ctx.scenario.client_entity_ref(&self.client_key, &user_key, key)
    }

    /// Get all entities as EntityKeys for this client
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.expect_ctx.scenario.entity_registry();
        registry.client_entity_keys(&self.client_key)
    }

    /// Get entity owner for an entity
    pub fn entity_owner(&mut self, entity: &EntityKey) -> Option<EntityOwner> {
        let registry = self.expect_ctx.scenario.entity_registry();
        let client_entity = registry.client_entity(entity, &self.client_key)?;
        let state = self.expect_ctx.scenario.client_state(&self.client_key);
        Some(state.client.entity_owner(&client_entity))
    }

    /// Get replication config for an entity
    pub fn entity_replication_config(&mut self, entity: &EntityKey) -> Option<ReplicationConfig> {
        let user_key = self.expect_ctx.scenario.user_key(&self.client_key);
        let local_entity = self.expect_ctx.scenario.local_entity_for(entity, &user_key)?;
        let state = self.expect_ctx.scenario.client_state_mut(&self.client_key);
        let world_ref = state.world.proxy();
        let client_ref = state.client.local_entity(world_ref, &local_entity)?;
        client_ref.replication_config()
    }

    /// Get authority status for an entity
    pub fn entity_authority_status(&mut self, entity: &EntityKey) -> Option<EntityAuthStatus> {
        let user_key = self.expect_ctx.scenario.user_key(&self.client_key);
        let local_entity = self.expect_ctx.scenario.local_entity_for(entity, &user_key)?;
        let state = self.expect_ctx.scenario.client_state_mut(&self.client_key);
        let world_ref = state.world.proxy();
        let client_ref = state.client.local_entity(world_ref, &local_entity)?;
        client_ref.authority()
    }

    /// Get connection status
    pub fn connection_status(&self) -> ConnectionStatus {
        let state = self.expect_ctx.scenario.client_state(&self.client_key);
        state.client.connection_status()
    }

    /// Get server address
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        let state = self.expect_ctx.scenario.client_state(&self.client_key);
        state.client.server_address()
    }
}



