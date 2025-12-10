use std::net::SocketAddr;

use naia_demo_world::WorldRef;
use naia_client::{EntityRef, ConnectionStatus, NaiaClientError, WorldEvents as ClientEvents};

use crate::{TestEntity, harness::{scenario::Scenario, EntityKey, ClientKey}};

/// Context for client-side expectations with per-tick events
pub struct ClientExpectCtx<'a> {
    scenario: &'a Scenario,
    client_key: ClientKey,
    events: &'a mut ClientEvents<TestEntity>,
}

impl<'a> ClientExpectCtx<'a> {
    pub(crate) fn new(
        scenario: &'a Scenario,
        client_key: ClientKey,
        events: &'a mut ClientEvents<TestEntity>,
    ) -> Self {
        Self {
            scenario,
            client_key,
            events,
        }
    }

    /// Access the per-tick client events
    /// 
    /// Events are consumed as they are read, following Naia's normal event semantics.
    pub fn events(&mut self) -> &mut ClientEvents<TestEntity> {
        self.events
    }

    pub fn has_entity(&self, entity: &EntityKey) -> bool {
        self.scenario.client_entity_ref(&self.client_key, entity).is_some()
    }
    
    /// Get read-only entity access by EntityKey
    /// Returns None if the entity doesn't exist or isn't visible to this client
    pub fn entity(&self, entity: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        self.scenario.client_entity_ref(&self.client_key, entity)
    }

    /// Get all entities as EntityKeys for this client
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.scenario.entity_registry();
        registry.client_entity_keys(&self.client_key)
    }

    /// Get server address
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        let state = self.scenario.client_state(&self.client_key);
        state.client().server_address()
    }
    
    /// Get connection status
    pub fn connection_status(&self) -> ConnectionStatus {
        let state = self.scenario.client_state(&self.client_key);
        state.client().connection_status()
    }
}



