use naia_demo_world::WorldRef;
use naia_server::{RoomKey, UserRef as NaiaUserRef};
use naia_shared::WorldRefType;

use crate::{
    harness::{
        room::RoomRef,
        scenario::Scenario,
        server_entity::ServerEntityRef,
        server_events::{ServerEvent, ServerEvents},
        user::UserRef,
        user_scope::UserScopeRef,
        ClientKey, EntityKey,
    },
    TestEntity,
};

/// Context for server-side expectations with per-tick events
pub struct ServerExpectCtx<'a> {
    scenario: &'a Scenario,
    events: &'a mut ServerEvents,
}

impl<'a> ServerExpectCtx<'a> {
    pub(crate) fn new(scenario: &'a Scenario, events: &'a mut ServerEvents) -> Self {
        Self { scenario, events }
    }

    pub fn scenario(&self) -> &Scenario {
        self.scenario
    }

    /// Get the server's current tick
    pub fn current_tick(&self) -> naia_shared::Tick {
        let (server, _) = self
            .scenario
            .server_and_registry()
            .expect("server not started");
        server.current_tick()
    }

    /// Read an event (returns first event if any)
    pub fn read_event<V: ServerEvent>(&mut self) -> Option<V::Item>
    where
        V::Iter: Iterator<Item = V::Item>,
    {
        self.events.read::<V>().next()
    }

    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&self, entity: &EntityKey) -> bool {
        self.scenario
            .entity_registry()
            .server_entity(entity)
            .is_some()
    }

    /// Get read-only entity access by EntityKey
    pub fn entity(&'_ self, key: &EntityKey) -> Option<ServerEntityRef<'_, WorldRef<'_>>> {
        let entity = self.scenario.entity_registry().server_entity(key)?;
        let (server, registry) = self.scenario.server_and_registry()?;
        let world_ref = self.scenario.server_world_ref();
        if !world_ref.has_entity(&entity) {
            return None;
        }
        let entity_ref = server.entity(world_ref, &entity);
        let users = self.scenario.client_users();
        Some(ServerEntityRef::new(entity_ref, users, registry))
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        // 1. Get UserKey via helper method
        let user_key = self.scenario.client_to_user_key(client_key)?;

        // 2. Get server and registry immutably
        let (server, registry) = self.scenario.server_and_registry()?;

        // 3. Call server.user_scope() to get the underlying scope
        let scope = server.user_scope(&user_key);

        // 4. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeRef::new(scope, registry))
    }

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let (server, registry) = self.scenario.server_and_registry().unwrap();
        let world_ref = self.scenario.server_world_ref();
        let server_entities = server.entities(world_ref);
        server_entities
            .iter()
            .filter_map(|e| registry.entity_key_for_server_entity(e))
            .collect()
    }

    /// True iff a Replicated Resource of type `R` is currently inserted
    /// on the server.
    pub fn has_resource<R: naia_shared::ReplicatedComponent>(&self) -> bool {
        let Some((server, _)) = self.scenario.server_and_registry() else {
            return false;
        };
        server.has_resource::<R>()
    }

    /// Read-only access to the value of a server-side Replicated Resource.
    /// The closure receives `Option<&R>`; `None` if `R` is not currently
    /// inserted.
    pub fn resource<R, F, T>(&self, f: F) -> Option<T>
    where
        R: naia_shared::ReplicatedComponent,
        F: FnOnce(&R) -> T,
    {
        let (server, _) = self.scenario.server_and_registry()?;
        let world_ref = self.scenario.server_world_ref();
        let entity = server.resource_entity::<R>()?;
        let comp_wrapper = world_ref.component::<R>(&entity)?;
        // ReplicaRefWrapper derefs to &R for read access
        Some(f(&*comp_wrapper))
    }

    /// Check if user exists for a ClientKey
    pub fn user_exists(&self, client_key: &ClientKey) -> bool {
        let Some(user_key) = self.scenario.client_to_user_key(client_key) else {
            return false;
        };
        let Some((server, _)) = self.scenario.server_and_registry() else {
            return false;
        };
        server.user_exists(&user_key)
    }

    /// Get read-only user access for a ClientKey
    pub fn user(&'_ self, client_key: &ClientKey) -> Option<UserRef<'_>> {
        let user_key = self.scenario.client_to_user_key(client_key)?;
        let users = self.scenario.client_users();
        let (server, _) = self.scenario.server_and_registry()?;
        let user: NaiaUserRef<'_, TestEntity> = server.user(&user_key);
        Some(UserRef::new(user, users))
    }

    /// Get all ClientKeys for connected users
    pub fn user_keys(&self) -> Vec<ClientKey> {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        let user_keys = server.user_keys();
        user_keys
            .iter()
            .filter_map(|uk| self.scenario.user_to_client_key(uk))
            .collect()
    }

    /// Get count of connected users
    pub fn users_count(&self) -> usize {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        server.users_count()
    }

    /// Check if room exists
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        server.room_exists(room_key)
    }

    /// Get read-only room access
    pub fn room(&'_ self, room_key: &RoomKey) -> Option<RoomRef<'_>> {
        let (server, registry) = self.scenario.server_and_registry()?;
        let users = self.scenario.client_users();
        if server.room_exists(room_key) {
            let room = server.room(room_key);
            Some(crate::harness::room::RoomRef::new(room, registry, users))
        } else {
            None
        }
    }

    /// Get all room keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        server.room_keys()
    }

    /// Get count of rooms
    pub fn rooms_count(&self) -> usize {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        server.rooms_count()
    }

    /// Read messages from a specific channel sent by clients
    /// Returns an iterator over (ClientKey, M) tuples for messages of type M received on channel C
    pub fn read_message<C: naia_shared::Channel, M: naia_shared::Message>(
        &mut self,
    ) -> impl Iterator<Item = (ClientKey, M)> {
        use naia_shared::{ChannelKind, MessageKind};

        let channel_kind = ChannelKind::of::<C>();
        let message_kind = MessageKind::of::<M>();

        // Access messages through a helper method on ServerEvents
        let messages = self
            .events
            .take_messages_for_channel_and_type(&channel_kind, &message_kind);

        messages.into_iter().map(|(client_key, container)| {
            let message: M =
                Box::<dyn std::any::Any + 'static>::downcast::<M>(container.to_boxed_any())
                    .ok()
                    .map(|boxed_m| *boxed_m)
                    .expect("Message type mismatch");
            (client_key, message)
        })
    }

    /// Read the sender-wide (global) priority gain override for an entity.
    /// Returns `None` when no override is in effect (default 1.0 applies).
    pub fn global_entity_gain(&self, entity_key: &EntityKey) -> Option<f32> {
        let entity = self.scenario.entity_registry().server_entity(entity_key)?;
        let (server, _) = self.scenario.server_and_registry()?;
        server.global_entity_priority(entity).gain()
    }

    /// True iff the sender-wide priority gain is explicitly overridden for
    /// this entity (i.e. the handle's `is_overridden()`).
    pub fn global_entity_priority_is_overridden(&self, entity_key: &EntityKey) -> bool {
        self.global_entity_gain(entity_key).is_some()
    }

    /// Read requests from a specific channel sent by clients
    /// Returns an iterator over (ClientKey, ResponseId, Request) tuples received on channel C
    pub fn read_request<C: naia_shared::Channel, Q: naia_shared::Request>(
        &mut self,
    ) -> impl Iterator<Item = (ClientKey, naia_shared::GlobalResponseId, Q)> {
        use naia_shared::{ChannelKind, MessageKind};
        let channel_kind = ChannelKind::of::<C>();
        let message_kind = MessageKind::of::<Q>();

        let requests = self
            .events
            .take_requests_for_channel_and_type(&channel_kind, &message_kind);

        requests
            .into_iter()
            .map(|(client_key, response_id, container)| {
                let request: Q =
                    Box::<dyn std::any::Any + 'static>::downcast::<Q>(container.to_boxed_any())
                        .ok()
                        .map(|boxed_q| *boxed_q)
                        .expect("Request type mismatch");
                (client_key, response_id, request)
            })
    }
}
