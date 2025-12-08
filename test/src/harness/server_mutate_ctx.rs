use naia_server::{EntityMut, EntityRef, EntityOwner, RoomKey, NaiaServerError, TickBufferMessages};
use naia_demo_world::{WorldRef, WorldMut};
use naia_shared::{Channel, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick};

use crate::{harness::{EntityKey, ClientKey, users::Users, entity_registry::EntityRegistry, user_scope::{UserScopeRef, UserScopeMut}, user::{UserRef, UserMut}, room::{RoomRef, RoomMut}}, TestEntity, TestWorld};

type Server = naia_server::Server<TestEntity>;

/// Lightweight handle for server-side mutations
/// Provides direct pass-through to core Server API with EntityKey resolution
pub struct ServerMutateCtx<'scenario> {
    server: &'scenario mut Server,
    world: &'scenario mut TestWorld,
    registry: &'scenario mut EntityRegistry,
    users: Users<'scenario>,
}

impl<'scenario> ServerMutateCtx<'scenario> {
    pub(crate) fn new(
        server: &'scenario mut Server,
        world: &'scenario mut TestWorld,
        registry: &'scenario mut EntityRegistry,
        users: Users<'scenario>,
    ) -> Self {
        Self {
            server,
            world,
            registry,
            users,
        }
    }

    /// Spawn a server entity, configure it, and return EntityKey
    pub fn spawn<F, R>(&mut self, f: F) -> (EntityKey, R)
    where
        F: for<'a> FnOnce(EntityMut<'a, TestEntity, WorldMut<'a>>) -> R,
    {
        // 1. Spawn entity via Server API
        let entity_mut = self.server.spawn_entity(self.world.proxy_mut());

        // 2. Allocate EntityKey
        let entity_key = self.registry.allocate_entity_key();

        // 3. Register server entity
        let entity = entity_mut.id();
        self.registry.register_server_entity(&entity_key, &entity);

        // 4. Call closure with EntityMut
        let result = f(entity_mut);

        // 5. Return (EntityKey, R)
        (entity_key, result)
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.server_entity(key)?;

        // 2. Get WorldRef with method lifetime
        let world_ref = self.world.proxy();

        // 3. Call server.entity() and return
        Some(self.server.entity(world_ref, &entity))
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.server_entity(key)?;

        // 2. Get WorldMut with method lifetime
        let world_mut = self.world.proxy_mut();

        // 3. Call server.entity_mut() and return
        Some(self.server.entity_mut(world_mut, &entity))
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&'_ self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        // 1. Get UserKey via users handle
        let user_key = self.users.user_for_client(*client_key)?;

        // 2. Call server.user_scope() to get the underlying scope
        let scope = self.server.user_scope(&user_key);

        // 3. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeRef::new(scope, self.registry))
    }

    /// Returns a HarnessUserScopeMut, which is used to include/exclude Entities for a
    /// given User. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope_mut(&'_ mut self, client_key: &ClientKey) -> Option<UserScopeMut<'_>> {
        // 1. Get UserKey via users handle
        let user_key = self.users.user_for_client(*client_key)?;

        // 2. Call server.user_scope_mut() to get the underlying scope
        let scope = self.server.user_scope_mut(&user_key);

        // 3. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeMut::new(scope, self.registry))
    }

    // Entity Operations

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let world_ref = self.world.proxy();
        let server_entities = self.server.entities(world_ref);
        server_entities.iter()
            .filter_map(|e| self.registry.entity_key_for_server_entity(e))
            .collect()
    }

    /// Get entity owner
    pub fn entity_owner(&self, entity: &EntityKey) -> Option<EntityOwner> {
        let entity = self.registry.server_entity(entity)?;
        Some(self.server.entity_owner(&entity))
    }

    // User Operations

    /// Check if user exists
    pub fn user_exists(&self, client_key: &ClientKey) -> bool {
        if let Some(user_key) = self.users.user_for_client(*client_key) {
            self.server.user_exists(&user_key)
        } else {
            false
        }
    }

    /// Get UserRef wrapper
    pub fn user(&'_ self, client_key: &ClientKey) -> Option<UserRef<'_>> {
        let user_key = self.users.user_for_client(*client_key)?;
        let user = self.server.user(&user_key);
        Some(UserRef::new(user, &self.users))
    }

    /// Get UserMut wrapper
    pub fn user_mut(&'_ mut self, client_key: &ClientKey) -> Option<UserMut<'_>> {
        let user_key = self.users.user_for_client(*client_key)?;
        let user = self.server.user_mut(&user_key);
        Some(UserMut::new(user, &self.users))
    }

    /// Get all user keys as ClientKeys
    pub fn user_keys(&self) -> Vec<ClientKey> {
        self.server.user_keys()
            .iter()
            .filter_map(|uk| self.users.client_for_user(uk))
            .collect()
    }

    /// Get user count
    pub fn users_count(&self) -> usize {
        self.server.users_count()
    }

    /// Accept connection
    pub fn accept_connection(&mut self, client_key: &ClientKey) {
        if let Some(user_key) = self.users.user_for_client(*client_key) {
            self.server.accept_connection(&user_key);
        }
    }

    /// Reject connection
    pub fn reject_connection(&mut self, client_key: &ClientKey) {
        if let Some(user_key) = self.users.user_for_client(*client_key) {
            self.server.reject_connection(&user_key);
        }
    }

    // Room Operations

    /// Create a new room
    pub fn make_room(&'_ mut self) -> RoomMut<'_> {
        let room = self.server.make_room();
        RoomMut::new(room, self.registry, &self.users)
    }

    /// Check if room exists
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.server.room_exists(room_key)
    }

    /// Get RoomRef wrapper
    pub fn room(&'_ self, room_key: &RoomKey) -> Option<RoomRef<'_>> {
        if self.server.room_exists(room_key) {
            let room = self.server.room(room_key);
            Some(RoomRef::new(room, self.registry, &self.users))
        } else {
            None
        }
    }

    /// Get RoomMut wrapper
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> Option<RoomMut<'_>> {
        if self.server.room_exists(room_key) {
            let room = self.server.room_mut(room_key);
            Some(RoomMut::new(room, self.registry, &self.users))
        } else {
            None
        }
    }

    /// Get all room keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.server.room_keys()
    }

    /// Get room count
    pub fn rooms_count(&self) -> usize {
        self.server.rooms_count()
    }

    // Message Operations

    /// Send message to user
    pub fn send_message<C: Channel, M: Message>(&mut self, client_key: &ClientKey, message: &M) {
        if let Some(user_key) = self.users.user_for_client(*client_key) {
            self.server.send_message::<C, M>(&user_key, message);
        }
    }

    /// Broadcast message to all users
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.server.broadcast_message::<C, M>(message);
    }

    /// Send request to user
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        client_key: &ClientKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        if let Some(user_key) = self.users.user_for_client(*client_key) {
            self.server.send_request::<C, Q>(&user_key, request)
        } else {
            Err(NaiaServerError::Message("user does not exist".to_string()))
        }
    }

    /// Send response
    pub fn send_response<S: Response>(&mut self, response_key: &ResponseSendKey<S>, response: &S) -> bool {
        self.server.send_response(response_key, response)
    }

    /// Receive response
    pub fn receive_response<S: Response>(&mut self, response_key: &ResponseReceiveKey<S>) -> Option<(ClientKey, S)> {
        self.server.receive_response(response_key)
            .and_then(|(user_key, response)| {
                self.users.client_for_user(&user_key)
                    .map(|client_key| (client_key, response))
            })
    }

    /// Receive tick-buffered messages
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.server.receive_tick_buffer_messages(tick)
    }
}

