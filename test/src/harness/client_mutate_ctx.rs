use std::net::SocketAddr;

use naia_client::{EntityMut, EntityRef, EntityOwner, NaiaClientError, ConnectionStatus};
use naia_shared::{Channel, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick};
use naia_demo_world::{WorldRef, WorldMut};
use naia_server::UserKey;

use crate::{harness::{ClientKey, EntityKey}, Scenario, TestEntity};

/// Lightweight handle for client-side mutations
/// Provides direct pass-through to core Client API with EntityKey resolution
pub struct ClientMutateCtx<'scenario> {
    scenario: &'scenario mut Scenario,
    client_key: ClientKey,
    user_key: UserKey,
}

impl<'scenario> ClientMutateCtx<'scenario> {
    pub(crate) fn new(
        scenario: &'scenario mut Scenario,
        client_key: ClientKey,
        user_key: UserKey,
    ) -> Self {
        // ClientMut holds &mut Scenario directly and borrows fields internally when needed
        Self {
            scenario,
            client_key,
            user_key,
        }
    }

    /// Spawn a client entity, configure it, wait for server registration, return EntityKey
    /// Synchronous: waits for server to have entity before returning
    pub fn spawn<F>(&mut self, f: F) -> EntityKey
    where
        F: for<'a> FnOnce(EntityMut<'a, TestEntity, WorldMut<'a>>),
    {
        // Use a single borrow of state and scoped blocks to manage borrows
        let state = self.scenario.client_state_mut(&self.client_key);
        
        // 1. Spawn entity via Client API
        // Get mutable references to both client and world
        let (client_mut, world_mut) = state.client_and_world_mut();
        let world_mut_proxy = world_mut.proxy_mut();
        let entity_mut = client_mut.spawn_entity(world_mut_proxy);
        
        // 2. Get entity ID and LocalEntity before closure consumes entity_mut
        let client_entity = entity_mut.id();
        let local_entity = entity_mut.local_entity()
            .expect("Client-spawned entity should have LocalEntity immediately");
        
        // 3. Call closure with EntityMut (this consumes entity_mut, dropping its borrows)
        f(entity_mut);
        // Now entity_mut is dropped, so we can borrow scenario again
        
        // 4. Allocate EntityKey
        let entity_key = self.scenario.entity_registry_mut().allocate_entity_key();

        // 5. Register spawning client's TestEntity and LocalEntity mapping immediately
        // This allows the server to look up the EntityKey when it receives the spawn event
        self.scenario.entity_registry_mut()
            .register_client_entity(&entity_key, &self.client_key, &client_entity, &local_entity);

        // 7. Return EntityKey - server entity will be registered automatically in tick_once()
        entity_key
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(
        &'_ self,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        self.scenario.client_entity_ref(&self.client_key, &self.user_key, key)
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        // The helper uses a single client_state_mut() call with scoped borrows
        self.scenario.client_entity_mut(&self.client_key, &self.user_key, key)
    }

    // Connection Operations

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

    /// Disconnect from server
    pub fn disconnect(&mut self) {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().disconnect();
    }

    // Entity Operations

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.scenario.entity_registry();
        // For client entities, we need to look them up via LocalEntity
        // Since we don't have LocalEntity here, use the registry's client_entity_keys method
        registry.client_entity_keys(&self.client_key)
    }

    /// Get entity owner
    pub fn entity_owner(&self, entity: &EntityKey) -> Option<EntityOwner> {
        let registry = self.scenario.entity_registry();
        let client_entity = registry.client_entity(entity, &self.client_key)?;
        let state = self.scenario.client_state(&self.client_key);
        Some(state.client().entity_owner(&client_entity))
    }

    // Message Operations

    /// Send message to server
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().send_message::<C, M>(message);
    }

    /// Send request to server
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaClientError> {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().send_request::<C, Q>(request)
    }

    /// Send response
    pub fn send_response<S: Response>(&mut self, response_key: &ResponseSendKey<S>, response: &S) -> bool {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().send_response(response_key, response)
    }

    /// Receive response
    pub fn receive_response<S: Response>(&mut self, response_key: &ResponseReceiveKey<S>) -> Option<S> {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().receive_response(response_key)
    }

    /// Send tick-buffered message
    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        let state = self.scenario.client_state_mut(&self.client_key);
        state.client_mut().send_tick_buffer_message::<C, M>(tick, message);
    }
}

