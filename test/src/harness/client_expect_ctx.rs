use std::net::SocketAddr;

use naia_demo_world::WorldRef;
use naia_client::{EntityRef, ConnectionStatus, NaiaClientError};

use crate::{TestEntity, harness::{ExpectCtx, EntityKey, ClientKey}};

/// Context for client-side expectations
pub struct ClientExpectCtx<'a, 'scenario: 'a> {
    ctx: &'a ExpectCtx<'scenario>,
    client_key: ClientKey,
}

impl<'a, 'scenario: 'a> ClientExpectCtx<'a, 'scenario> {
    pub(crate) fn new(ctx: &'a ExpectCtx<'scenario>, client_key: ClientKey) -> Self {
        Self { ctx, client_key }
    }

    pub fn has_entity(&self, entity: &EntityKey) -> bool {
        self.ctx.scenario().client_entity_ref(&self.client_key, entity).is_some()
    }
    
    /// Get read-only entity access by EntityKey
    /// Returns None if the entity doesn't exist or isn't visible to this client
    pub fn entity(&self, entity: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        self.ctx.scenario().client_entity_ref(&self.client_key, entity)
    }

    /// Get all entities as EntityKeys for this client
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.ctx.scenario().entity_registry();
        registry.client_entity_keys(&self.client_key)
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



