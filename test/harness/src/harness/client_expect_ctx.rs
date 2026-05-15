use std::net::SocketAddr;

use naia_client::{ConnectionStatus, NaiaClientError};
use naia_demo_world::WorldRef;

use crate::harness::{
    client_events::{ClientEvent, ClientEvents, ClientRejectEvent},
    scenario::Scenario,
    ClientEntityRef, ClientKey, EntityKey,
};

/// Context for client-side expectations with per-tick events
pub struct ClientExpectCtx<'a> {
    scenario: &'a Scenario,
    client_key: ClientKey,
    events: &'a mut ClientEvents,
}

impl<'a> ClientExpectCtx<'a> {
    pub(crate) fn new(
        scenario: &'a Scenario,
        client_key: ClientKey,
        events: &'a mut ClientEvents,
    ) -> Self {
        Self {
            scenario,
            client_key,
            events,
        }
    }

    /// Read an event (returns first event if any)
    pub fn read_event<V: ClientEvent>(&mut self) -> Option<V::Item>
    where
        V::Iter: Iterator<Item = V::Item>,
    {
        self.events.read::<V>().next()
    }

    pub fn has_entity(&self, entity: &EntityKey) -> bool {
        self.scenario
            .client_entity_ref(&self.client_key, entity)
            .is_some()
    }

    /// Get read-only entity access by EntityKey
    /// Returns None if the entity doesn't exist or isn't visible to this client
    pub fn entity(&self, entity: &EntityKey) -> Option<ClientEntityRef<'_, WorldRef<'_>>> {
        self.scenario.client_entity_ref(&self.client_key, entity)
    }

    /// Get all entities as EntityKeys for this client
    pub fn entities(&self) -> Vec<EntityKey> {
        let registry = self.scenario.entity_registry();
        registry.client_entity_keys(&self.client_key)
    }

    /// True iff the client's world contains a Replicated Resource of
    /// type `R`.
    pub fn has_resource<R: naia_shared::ReplicatedComponent>(&self) -> bool {
        let state = self.scenario.client_state(&self.client_key);
        let world_ref = state.world().proxy();
        crate::harness::resource_lookup::has_resource_in_world::<R, _>(&world_ref)
    }

    /// Read the value of a client-side Replicated Resource. The closure
    /// receives `&R`. Returns `None` if not present in this client's
    /// world.
    pub fn resource<R, F, T>(&self, f: F) -> Option<T>
    where
        R: naia_shared::ReplicatedComponent,
        F: FnOnce(&R) -> T,
    {
        let state = self.scenario.client_state(&self.client_key);
        let world_ref = state.world().proxy();
        crate::harness::resource_lookup::read_resource_in_world::<R, _, _, _>(&world_ref, f)
    }

    /// Read this client's view of a resource's authority status.
    /// Returns `None` if the resource is not present in this client's
    /// world or if it isn't delegable.
    pub fn resource_authority_status<R: naia_shared::ReplicatedComponent>(
        &self,
    ) -> Option<naia_shared::EntityAuthStatus> {
        let state = self.scenario.client_state(&self.client_key);
        let world_ref = state.world().proxy();
        use naia_shared::WorldRefType;
        for e in world_ref.entities() {
            if world_ref.has_component::<R>(&e) {
                return state.client().entity_authority_status(&e);
            }
        }
        None
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

    /// Get the last identity token provided by the server to this client
    /// Returns None if no token has been received yet
    pub fn identity_token(&self) -> Option<naia_shared::IdentityToken> {
        let state = self.scenario.client_state(&self.client_key);
        state.identity_token()
    }

    /// Get the last rejection code (if any) returned by the handshake
    /// Returns None if no rejection occurred
    pub fn rejection_code(&self) -> Option<u16> {
        let state = self.scenario.client_state(&self.client_key);
        state.rejection_code()
    }

    /// Check if the client was explicitly rejected by the server
    ///
    /// Returns true if:
    /// - A rejection code (typically 401) was received, OR
    /// - A RejectEvent is present in the current tick's events
    pub fn is_rejected(&self) -> bool {
        // Check if rejection code is present (401 or other error codes)
        if let Some(code) = self.rejection_code() {
            // 401 is the standard rejection code, but other error codes also indicate rejection
            if code == 401 || code >= 400 {
                return true;
            }
        }

        // Check if RejectEvent is present in current events
        self.events.has::<ClientRejectEvent>()
    }

    /// Check if an event type is present
    pub fn has<V: ClientEvent>(&self) -> bool {
        self.events.has::<V>()
    }

    /// Check if any component insert event was received for the given entity this tick or earlier
    pub fn has_insert_event_for_entity(&self, entity_key: &EntityKey) -> bool {
        self.events.has_insert_for_entity(entity_key)
            || self.scenario.client_insert_ever_fired(&self.client_key, entity_key)
    }

    /// Check if any component update event was received for the given entity this tick
    pub fn has_update_event_for_entity(&self, entity_key: &EntityKey) -> bool {
        self.events.has_update_for_entity(entity_key)
    }

    /// Check if any component remove event was received for the given entity this tick or earlier
    pub fn has_remove_event_for_entity(&self, entity_key: &EntityKey) -> bool {
        self.events.has_remove_for_entity(entity_key)
            || self.scenario.client_remove_ever_fired(&self.client_key, entity_key)
    }

    /// Read messages from a specific channel
    /// Returns an iterator over messages of type M received on channel C
    pub fn read_message<C: naia_shared::Channel, M: naia_shared::Message>(
        &mut self,
    ) -> impl Iterator<Item = M> {
        use naia_shared::{ChannelKind, MessageKind};

        let channel_kind = ChannelKind::of::<C>();
        let message_kind = MessageKind::of::<M>();

        // Access messages through a helper method on ClientEvents
        let messages = self
            .events
            .take_messages_for_channel_and_type(&channel_kind, &message_kind);

        messages.into_iter().map(|container| {
            Box::<dyn std::any::Any + 'static>::downcast::<M>(container.to_boxed_any())
                .ok()
                .map(|boxed_m| *boxed_m)
                .expect("Message type mismatch")
        })
    }

    /// Read requests from a specific channel
    /// Returns an iterator over (ResponseId, Request) tuples received on channel C
    pub fn read_request<C: naia_shared::Channel, Q: naia_shared::Request>(
        &mut self,
    ) -> impl Iterator<Item = (naia_shared::GlobalResponseId, Q)> {
        use naia_shared::{ChannelKind, MessageKind};
        let channel_kind = ChannelKind::of::<C>();
        let message_kind = MessageKind::of::<Q>();

        let requests = self
            .events
            .take_requests_for_channel_and_type(&channel_kind, &message_kind);

        requests.into_iter().map(|(response_id, container)| {
            let request: Q =
                Box::<dyn std::any::Any + 'static>::downcast::<Q>(container.to_boxed_any())
                    .ok()
                    .map(|boxed_q| *boxed_q)
                    .expect("Request type mismatch");
            (response_id, request)
        })
    }

    /// Check if a response is available for the given request key (non-destructive)
    pub fn has_response<S: naia_shared::Response>(
        &self,
        response_key: &naia_shared::ResponseReceiveKey<S>,
    ) -> bool {
        let state = self.scenario.client_state(&self.client_key);
        state.client().has_response(response_key)
    }

    /// Get the server tick that this client has received and processed
    /// (after jitter buffer)
    /// This is the tick of server updates that have been received and processed.
    pub fn server_tick(&self) -> Option<naia_shared::Tick> {
        let state = self.scenario.client_state(&self.client_key);
        state.client().server_tick()
    }

    /// Get the client's predicted tick (how far ahead client is predicting)
    /// This is the client's internal prediction tick for client-side prediction.
    pub fn client_tick(&self) -> Option<naia_shared::Tick> {
        let state = self.scenario.client_state(&self.client_key);
        state.client().client_tick()
    }

    /// Get the round-trip time (RTT) estimate in seconds
    /// This is an observability metric per [observability-03]
    pub fn rtt(&self) -> f32 {
        let state = self.scenario.client_state(&self.client_key);
        state.client().rtt()
    }

    /// Get outgoing bandwidth in kbps
    /// This is an observability metric per [observability-05], [observability-06]
    pub fn outgoing_bandwidth(&self) -> f32 {
        let state = self.scenario.client_state(&self.client_key);
        state.client().outgoing_bandwidth()
    }

    /// Get incoming bandwidth in kbps
    /// This is an observability metric per [observability-05], [observability-06]
    pub fn incoming_bandwidth(&self) -> f32 {
        let state = self.scenario.client_state(&self.client_key);
        state.client().incoming_bandwidth()
    }

    /// Check if this client is connected to the server
    pub fn is_connected(&self) -> bool {
        self.connection_status() == ConnectionStatus::Connected
    }
}
