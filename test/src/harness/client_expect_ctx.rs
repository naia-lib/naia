use std::net::SocketAddr;

use naia_demo_world::WorldRef;
use naia_client::{EntityRef, ConnectionStatus, NaiaClientError, EntityOwner};

use crate::{TestEntity, harness::{ExpectCtx, EntityKey, ClientKey}};

/// Context for client-side expectations
pub struct ClientExpectCtx<'b, 'a: 'b> {
    ctx: &'b mut ExpectCtx<'a>,
    client_key: ClientKey,
}

impl<'b, 'a: 'b> ClientExpectCtx<'b, 'a> {
    pub(crate) fn new(expect_ctx: &'b mut ExpectCtx<'a>, client_key: ClientKey) -> Self {
        Self { ctx: expect_ctx, client_key }
    }

    pub fn has_entity(&mut self, entity: &EntityKey) -> bool {
        let user_key = self.ctx.scenario().user_key(&self.client_key);
        self.ctx.scenario().client_entity_ref(&self.client_key, &user_key, entity).is_some()
    }
    
    /// Get read-only entity access by EntityKey
    /// Returns None if the entity doesn't exist or isn't visible to this client
    pub fn entity(&'_ mut self, entity: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let user_key = self.ctx.scenario().user_key(&self.client_key);
        self.ctx.scenario().client_entity_ref(&self.client_key, &user_key, entity)
    }

    /// Get all entities as EntityKeys for this client
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.ctx.scenario().entity_registry();
        registry.client_entity_keys(&self.client_key)
    }

    /// Get entity owner for an entity
    pub fn entity_owner(&mut self, entity: &EntityKey) -> Option<EntityOwner> {
        let registry = self.ctx.scenario().entity_registry();
        let client_entity = registry.client_entity(entity, &self.client_key)?;
        let state = self.ctx.scenario().client_state(&self.client_key);
        Some(state.client().entity_owner(&client_entity))
    }

    /// Get server address
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        let state = self.ctx.scenario().client_state(&self.client_key);
        state.client().server_address()
    }
    
    /// Get connection status
    pub fn connection_status(&self) -> ConnectionStatus {
        let state = self.ctx.scenario().client_state(&self.client_key);
        state.client().connection_status()
    }
}



