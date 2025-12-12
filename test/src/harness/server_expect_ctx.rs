use naia_server::{EntityRef, UserRef, RoomRef, RoomKey, Events as ServerEvents};
use naia_demo_world::WorldRef;

use crate::{harness::{events::TranslatableEvent, scenario::Scenario, user_scope::UserScopeRef, EntityKey, ClientKey}, TestEntity};

/// Context for server-side expectations with per-tick events
pub struct ServerExpectCtx<'a> {
    scenario: &'a Scenario,
    events: &'a mut ServerEvents<TestEntity>,
}

impl<'a> ServerExpectCtx<'a> {
    pub(crate) fn new(
        scenario: &'a Scenario,
        events: &'a mut ServerEvents<TestEntity>,
    ) -> Self {
        Self {
            scenario,
            events,
        }
    }

    pub fn scenario(&self) -> &Scenario {
        self.scenario
    }

    /// Read a translated event
    pub fn read_event<E>(&mut self) -> Option<E::Output>
    where
        E: TranslatableEvent<ServerExpectCtx<'a>> + naia_server::Event<TestEntity>,
        <E as naia_server::Event<TestEntity>>::Iter: Iterator<Item = <E as TranslatableEvent<ServerExpectCtx<'a>>>::Input>,
    {
        self.events.read::<E>().find_map(|event| E::translate_item(self, event))
    }

    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&self, entity: &EntityKey) -> bool {
        self.scenario.entity_registry().server_entity(entity).is_some()
    }

    /// Get read-only entity access by EntityKey
    pub fn entity(&'_ self, key: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let entity = self.scenario.entity_registry().server_entity(key)?;
        let (server, _) = self.scenario.server_and_registry()?;
        let world_ref = self.scenario.server_world_ref();
        Some(server.entity(world_ref, &entity))
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
        server_entities.iter()
            .filter_map(|e| registry.entity_key_for_server_entity(e))
            .collect()
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
    pub fn user(&'_ self, client_key: &ClientKey) -> Option<UserRef<'_, TestEntity>> {
        let user_key = self.scenario.client_to_user_key(client_key)?;
        let (server, _) = self.scenario.server_and_registry()?;
        Some(server.user(&user_key))
    }

    /// Get all ClientKeys for connected users
    pub fn user_keys(&self) -> Vec<ClientKey> {
        let (server, _) = self.scenario.server_and_registry().unwrap();
        let user_keys = server.user_keys();
        user_keys.iter()
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
    pub fn room(&'_ self, room_key: &RoomKey) -> Option<RoomRef<'_, TestEntity>> {
        let (server, _) = self.scenario.server_and_registry()?;
        Some(server.room(room_key))
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
}