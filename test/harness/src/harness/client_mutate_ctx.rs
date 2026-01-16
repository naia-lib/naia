use std::net::SocketAddr;

use naia_client::{ConnectionStatus, NaiaClientError};
use naia_demo_world::{WorldMut, WorldRef};
use naia_shared::{
    Channel, IdentityToken, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick,
};

use crate::{
    harness::{ClientEntityMut, ClientEntityRef, mutate_ctx::MutateCtx, ClientKey, EntityKey},
};

/// Lightweight handle for client-side mutations
/// Provides direct pass-through to core Client API with EntityKey resolution
pub struct ClientMutateCtx<'a, 'scenario: 'a> {
    ctx: &'a mut MutateCtx<'scenario>,
    client_key: ClientKey,
}

impl<'a, 'scenario: 'a> ClientMutateCtx<'a, 'scenario> {
    pub(crate) fn new(ctx: &'a mut MutateCtx<'scenario>, client_key: ClientKey) -> Self {
        Self { ctx, client_key }
    }

    /// Spawn a client entity, configure it, wait for server registration, return EntityKey
    /// Synchronous: waits for server to have entity before returning
    pub fn spawn<F>(&mut self, f: F) -> EntityKey
    where
        F: for<'b> FnOnce(ClientEntityMut<'b, WorldMut<'b>>),
    {
        let scenario = self.ctx.scenario_mut();
        // Create Users the same way split_for_server_mut does
        let (state, registry) = scenario.split_for_client_mut(&self.client_key)
            .expect("client state not found");

        // 1. Spawn entity via Client API
        // Get mutable references to both client and world
        let (client_mut, world_mut) = state.client_and_world_mut();
        let world_mut_proxy = world_mut.proxy_mut();
        let entity_mut = client_mut.spawn_entity(world_mut_proxy);

        // 2. Get entity ID and LocalEntity before closure consumes entity_mut
        let client_entity = entity_mut.id();
        let local_entity = entity_mut
            .local_entity()
            .expect("Client-spawned entity should have LocalEntity immediately");

        // 3. Wrap EntityMut in ClientEntityMut and call closure (this consumes entity_mut, dropping its borrows)
        // Reborrow registry as immutable for ClientEntityMut::new
        let client_entity_mut = ClientEntityMut::new(entity_mut, &*registry, self.client_key);
        f(client_entity_mut);
        // Now entity_mut is dropped, so we can borrow scenario again

        // 4. Allocate EntityKey
        let entity_key = self
            .ctx
            .scenario_mut()
            .entity_registry_mut()
            .allocate_entity_key();

        // 5. Register spawning client's TestEntity and LocalEntity mapping immediately
        // This allows the server to look up the EntityKey when it receives the spawn event
        self.ctx
            .scenario_mut()
            .entity_registry_mut()
            .register_client_entity(&entity_key, &self.client_key, &client_entity, &local_entity);

        // 7. Return EntityKey - server entity will be registered automatically in tick_once()
        entity_key
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(&'_ self, entity: &EntityKey) -> Option<ClientEntityRef<'_, WorldRef<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        self.ctx
            .scenario()
            .client_entity_ref(&self.client_key, entity)
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        entity: &EntityKey,
    ) -> Option<ClientEntityMut<'_, WorldMut<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        // The helper uses a single client_state_mut() call with scoped borrows
        self.ctx
            .scenario_mut()
            .client_entity_mut(&self.client_key, entity)
    }

    // Connection Operations

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

    /// Disconnect from server
    pub fn disconnect(&mut self) {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state.client_mut().disconnect();
    }

    // Entity Operations

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.ctx.scenario().entity_registry();
        // For client entities, we need to look them up via LocalEntity
        // Since we don't have LocalEntity here, use the registry's client_entity_keys method
        registry.client_entity_keys(&self.client_key)
    }

    // Message Operations

    /// Send message to server
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state.client_mut().send_message::<C, M>(message);
    }

    /// Send request to server
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaClientError> {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state.client_mut().send_request::<C, Q>(request)
    }

    /// Send response
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state.client_mut().send_response(response_key, response)
    }

    /// Receive response
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<S> {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state.client_mut().receive_response(response_key)
    }

    /// Send tick-buffered message
    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        state
            .client_mut()
            .send_tick_buffer_message::<C, M>(tick, message);
    }

    /// Manually set the identity token for this client
    ///
    /// This allows tests to inject a token before connecting, or to tamper with/reuse
    /// a token for negative testing scenarios. The token will be used when the client
    /// attempts to connect.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Set a token before connecting
    /// scenario.mutate(|ctx| {
    ///     ctx.client(client_key, |c| {
    ///         c.set_identity_token("test_token_123".to_string());
    ///     });
    /// });
    ///
    /// // Tamper with a received token
    /// scenario.mutate(|ctx| {
    ///     ctx.client(client_key, |c| {
    ///         if let Some(token) = c.identity_token() {
    ///             let tampered = format!("{}_tampered", token);
    ///             c.set_identity_token(tampered);
    ///         }
    ///     });
    /// });
    /// ```
    pub fn set_identity_token(&mut self, token: IdentityToken) {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        *state.identity_token_handle().lock().unwrap() = Some(token);
    }

    /// Get the current identity token (if any) for this client
    ///
    /// Returns the token that was either:
    /// - Received from the server during handshake
    /// - Manually set via `set_identity_token()`
    ///
    /// Returns None if no token has been set or received yet.
    pub fn identity_token(&self) -> Option<IdentityToken> {
        let state = self.ctx.scenario().client_state(&self.client_key);
        state.identity_token()
    }

    /// Clear the identity token for this client
    ///
    /// Useful for testing scenarios where you want to simulate a client
    /// without a token or reset the token state.
    pub fn clear_identity_token(&mut self) {
        let state = self.ctx.scenario_mut().client_state_mut(&self.client_key);
        *state.identity_token_handle().lock().unwrap() = None;
    }

    /// Get the server tick that this client has received and processed
    /// (after jitter buffer)
    /// This is the tick of server updates that have been received and processed.
    pub fn server_tick(&self) -> Option<Tick> {
        let state = self.ctx.scenario().client_state(&self.client_key);
        state.client().server_tick()
    }

    /// Get the client's predicted tick (how far ahead client is predicting)
    /// This is the client's internal prediction tick for client-side prediction.
    pub fn client_tick(&self) -> Option<Tick> {
        let state = self.ctx.scenario().client_state(&self.client_key);
        state.client().client_tick()
    }

}
