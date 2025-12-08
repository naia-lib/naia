use naia_server::{EntityRef, Event, EntityOwner, UserRef, RoomRef, RoomKey};
use naia_demo_world::WorldRef;

use crate::{harness::{ExpectCtx, user_scope::UserScopeRef, EntityKey, ClientKey}, TestEntity};

/// Context for server-side expectations
pub struct ServerExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
}

impl<'b, 'a: 'b> ServerExpectCtx<'b, 'a> {
    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&mut self, entity: &EntityKey) -> bool {
        self.expect_ctx.scenario.server_host_entity(entity).is_some()
    }

    /// Get read-only entity access by EntityKey
    pub fn entity(&'_ self, key: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let scenario = &self.expect_ctx.scenario;
        let entity = scenario.entity_registry().server_entity(key)?;
        let (server, _) = scenario.server_and_registry()?;
        let world_ref = scenario.server_world_ref();
        Some(server.entity(world_ref, &entity))
    }

    /// Expect that the server will produce at least one event of type T
    /// T must implement Event<TestEntity>
    pub fn event<T>(&mut self, _label: &str) -> bool 
    where
        T: Event<TestEntity>,
        <T as Event<TestEntity>>::Iter: Iterator,
    {
        let mut events = self.expect_ctx.scenario.take_server_events();
        for _ in events.read::<T>() {
            return true;
        }
        false
    }

    /// Check if server has at least one event of type T
    /// T must implement Event<TestEntity>
    /// Note: This consumes events via take_server_events()
    pub fn has_event<T: Event<TestEntity>>(&mut self) -> bool {
        let events = self.expect_ctx.scenario.take_server_events();
        events.has::<T>()
    }

    /// Count occurrences of event type T
    /// T must implement Event<TestEntity>
    /// Note: This consumes events via take_server_events()
    pub fn event_count<T>(&mut self) -> usize 
    where
        T: Event<TestEntity>,
        <T as Event<TestEntity>>::Iter: Iterator,
    {
        let mut events = self.expect_ctx.scenario.take_server_events();
        let mut count = 0;
        for _ in events.read::<T>() {
            count += 1;
        }
        count
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        let scenario = &self.expect_ctx.scenario;
        
        // 1. Get UserKey via helper method
        let user_key = scenario.user_key_for_client(client_key)?;

        // 2. Get server and registry immutably
        let (server, registry) = scenario.server_and_registry()?;

        // 3. Call server.user_scope() to get the underlying scope
        let scope = server.user_scope(&user_key);

        // 4. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeRef::new(scope, registry))
    }

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let scenario = &self.expect_ctx.scenario;
        let (server, registry) = scenario.server_and_registry().unwrap();
        let world_ref = scenario.server_world_ref();
        let server_entities = server.entities(world_ref);
        server_entities.iter()
            .filter_map(|e| registry.entity_key_for_server_entity(e))
            .collect()
    }

    /// Get entity owner for an entity
    pub fn entity_owner(&self, entity: &EntityKey) -> Option<EntityOwner> {
        let scenario = &self.expect_ctx.scenario;
        let server_entity = scenario.entity_registry().server_entity(entity)?;
        let (server, _) = scenario.server_and_registry()?;
        Some(server.entity_owner(&server_entity))
    }

    /// Check if user exists for a ClientKey
    pub fn user_exists(&self, client_key: &ClientKey) -> bool {
        let scenario = &self.expect_ctx.scenario;
        let Some(user_key) = scenario.user_key_for_client(client_key) else {
            return false;
        };
        let Some((server, _)) = scenario.server_and_registry() else {
            return false;
        };
        server.user_exists(&user_key)
    }

    /// Get read-only user access for a ClientKey
    pub fn user(&'_ self, client_key: &ClientKey) -> Option<UserRef<'_, TestEntity>> {
        let scenario = &self.expect_ctx.scenario;
        let user_key = scenario.user_key_for_client(client_key)?;
        let (server, _) = scenario.server_and_registry()?;
        Some(server.user(&user_key))
    }

    /// Get all ClientKeys for connected users
    pub fn user_keys(&self) -> Vec<ClientKey> {
        let scenario = &self.expect_ctx.scenario;
        let (server, _) = scenario.server_and_registry().unwrap();
        let user_keys = server.user_keys();
        user_keys.iter()
            .filter_map(|uk| scenario.client_key_for_user(uk))
            .collect()
    }

    /// Get count of connected users
    pub fn users_count(&self) -> usize {
        let (server, _) = self.expect_ctx.scenario.server_and_registry().unwrap();
        server.users_count()
    }

    /// Check if room exists
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        let (server, _) = self.expect_ctx.scenario.server_and_registry().unwrap();
        server.room_exists(room_key)
    }

    /// Get read-only room access
    pub fn room(&'_ self, room_key: &RoomKey) -> Option<RoomRef<'_, TestEntity>> {
        let (server, _) = self.expect_ctx.scenario.server_and_registry()?;
        Some(server.room(room_key))
    }

    /// Get all room keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        let (server, _) = self.expect_ctx.scenario.server_and_registry().unwrap();
        server.room_keys()
    }

    /// Get count of rooms
    pub fn rooms_count(&self) -> usize {
        let (server, _) = self.expect_ctx.scenario.server_and_registry().unwrap();
        server.rooms_count()
    }
}