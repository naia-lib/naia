use log::warn;

use naia_demo_world::{WorldMut, WorldRef};
use naia_server::{EntityMut, EntityRef, NaiaServerError, RoomKey, TickBufferMessages};
use naia_shared::WorldRefType;
use naia_shared::{
    generate_identity_token, Channel, IdentityToken, Message, Request, Response,
    ResponseReceiveKey, ResponseSendKey, Tick,
};

use crate::{
    harness::{
        mutate_ctx::MutateCtx,
        room::{RoomMut, RoomRef},
        user::{UserMut, UserRef},
        user_scope::{UserScopeMut, UserScopeRef},
        ClientKey, EntityKey,
    },
    TestEntity,
};

/// Lightweight handle for server-side mutations
/// Provides direct pass-through to core Server API with EntityKey resolution
pub struct ServerMutateCtx<'a, 'scenario: 'a> {
    ctx: &'a mut MutateCtx<'scenario>,
}

impl<'a, 'scenario: 'a> ServerMutateCtx<'a, 'scenario> {
    pub(crate) fn new(ctx: &'a mut MutateCtx<'scenario>) -> Self {
        Self { ctx }
    }

    /// Spawn a server entity, configure it, and return EntityKey
    pub fn spawn<F, R>(&mut self, f: F) -> (EntityKey, R)
    where
        F: for<'b> FnOnce(EntityMut<'b, TestEntity, WorldMut<'b>>) -> R,
    {
        let scenario = self.ctx.scenario_mut();
        let (server, world, registry, _) = scenario.split_for_server_mut();

        // 1. Spawn entity via Server API
        let entity_mut = server.spawn_entity(world.proxy_mut());

        // 2. Allocate EntityKey
        let entity_key = registry.allocate_entity_key();

        // 3. Register server entity
        let entity = entity_mut.id();
        registry.register_server_entity(&entity_key, &entity);

        // 4. Call closure with EntityMut
        let result = f(entity_mut);

        // 5. Return (EntityKey, R)
        (entity_key, result)
    }

    /// Get the server's current tick
    pub fn current_tick(&self) -> Tick {
        let scenario = self.ctx.scenario();
        let (server, _) = scenario.server_and_registry().expect("server not started");
        server.current_tick()
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(&'_ self, key: &EntityKey) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        let scenario = self.ctx.scenario();
        let entity = scenario.entity_registry().server_entity(key)?;
        let (server, _) = scenario.server_and_registry()?;
        let world_ref = scenario.server_world_ref();
        Some(server.entity(world_ref, &entity))
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        let scenario = self.ctx.scenario_mut();
        let entity = scenario.entity_registry().server_entity(key)?;
        let (server, world, _, _) = scenario.split_for_server_mut();
        let world_mut = world.proxy_mut();
        if !world_mut.has_entity(&entity) {
            return None;
        }
        Some(server.entity_mut(world_mut, &entity))
    }

    /// Despawn an entity by EntityKey
    pub fn despawn(&mut self, key: &EntityKey) {
        if let Some(mut entity_mut) = self.entity_mut(key) {
            entity_mut.despawn();
            self.ctx
                .scenario_mut()
                .entity_registry_mut()
                .unregister_server_entity(key);
        }
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&'_ self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        let scenario = self.ctx.scenario();
        let users = scenario.client_users();
        let user_key = users.client_to_user_key(client_key)?;
        let (server, registry) = scenario.server_and_registry()?;
        let scope = server.user_scope(&user_key);
        Some(UserScopeRef::new(scope, registry))
    }

    /// Returns a HarnessUserScopeMut, which is used to include/exclude Entities for a
    /// given User. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope_mut(&'_ mut self, client_key: &ClientKey) -> Option<UserScopeMut<'_>> {
        let scenario = self.ctx.scenario_mut();
        let user_key = scenario.client_to_user_key(client_key)?;
        let (server, _, registry, _) = scenario.split_for_server_mut();
        let scope = server.user_scope_mut(&user_key);
        Some(UserScopeMut::new(scope, registry))
    }

    // Entity Operations

    /// Get all entities as EntityKeys
    pub fn entities(&self) -> Vec<EntityKey> {
        let scenario = self.ctx.scenario();
        let (server, registry) = scenario.server_and_registry().unwrap();
        let world_ref = scenario.server_world_ref();
        let server_entities = server.entities(world_ref);
        server_entities
            .iter()
            .filter_map(|e| registry.entity_key_for_server_entity(e))
            .collect()
    }

    // User Operations

    /// Check if user exists
    pub fn user_exists(&self, client_key: &ClientKey) -> bool {
        let scenario = self.ctx.scenario();
        let users = scenario.client_users();
        if let Some(user_key) = users.client_to_user_key(client_key) {
            if let Some((server, _)) = scenario.server_and_registry() {
                server.user_exists(&user_key)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get UserRef wrapper
    pub fn user(&'_ self, client_key: &ClientKey) -> Option<UserRef<'_>> {
        let scenario = self.ctx.scenario();
        let user_key = scenario.client_to_user_key(client_key)?;
        let users = scenario.client_users();
        let (server, _) = scenario.server_and_registry()?;
        let user = server.user(&user_key);
        Some(UserRef::new(user, users))
    }

    /// Get UserMut wrapper
    pub fn user_mut(&'_ mut self, client_key: &ClientKey) -> Option<UserMut<'_>> {
        let scenario = self.ctx.scenario_mut();
        let user_key = scenario.client_to_user_key(client_key)?;
        let (server, _, _, users) = scenario.split_for_server_mut();
        let user = server.user_mut(&user_key);
        Some(UserMut::new(user, users))
    }

    /// Get all user keys as ClientKeys
    pub fn user_keys(&self) -> Vec<ClientKey> {
        let scenario = self.ctx.scenario();
        let (server, _) = scenario.server_and_registry().unwrap();
        let users = scenario.client_users();
        server
            .user_keys()
            .iter()
            .filter_map(|uk| users.user_to_client_key(uk))
            .collect()
    }

    /// Get user count
    pub fn users_count(&self) -> usize {
        let (server, _) = self.ctx.scenario().server_and_registry().unwrap();
        server.users_count()
    }

    /// Accept connection for a client
    ///
    /// Requires that the ClientKey has been mapped to a UserKey (via reading AuthEvent).
    /// Panics if the mapping doesn't exist.
    pub fn accept_connection(&mut self, client_key: &ClientKey) {
        let scenario = self.ctx.scenario_mut();
        let user_key = scenario.client_to_user_key(client_key).unwrap();
        let (server, _, _, _) = scenario.split_for_server_mut();
        server.accept_connection(&user_key);
    }

    /// Reject connection
    ///
    /// # Note
    ///
    /// This method silently fails if the ClientKey has no associated UserKey
    /// (e.g., if the client hasn't authenticated yet). A warning is logged in this case.
    pub fn reject_connection(&mut self, client_key: &ClientKey) {
        let scenario = self.ctx.scenario_mut();
        if let Some(user_key) = scenario.client_to_user_key(client_key) {
            let (server, _, _, _) = scenario.split_for_server_mut();
            server.reject_connection(&user_key);
        } else {
            warn!("reject_connection failed: ClientKey {:?} has no associated UserKey (may not be authenticated yet)", client_key);
        }
    }

    /// Disconnect a user from the server
    ///
    /// This requests a server-side disconnect of the user identified by the given ClientKey.
    /// The user will be disconnected in the next tick.
    ///
    /// # Returns
    ///
    /// Returns `true` if the disconnect was queued successfully, `false` if the ClientKey
    /// has no associated UserKey (e.g., not authenticated yet or already disconnected).
    ///
    /// # Note
    ///
    /// A warning is logged if the operation fails, which can help diagnose test issues.
    pub fn disconnect_user(&mut self, client_key: &ClientKey) -> bool {
        // Use the user_mut() method to get UserMut and call disconnect on it
        // This handles the ClientKey -> UserKey conversion internally
        if let Some(mut user) = self.user_mut(client_key) {
            user.disconnect();
            true
        } else {
            warn!("disconnect_user failed: ClientKey {:?} has no associated UserKey (may not be authenticated yet or already disconnected)", client_key);
            false
        }
    }

    // Room Operations

    /// Create a new room
    pub fn make_room(&'_ mut self) -> RoomMut<'_> {
        let scenario = self.ctx.scenario_mut();
        let (server, _, registry, users) = scenario.split_for_server_mut();
        let room = server.make_room();
        RoomMut::new(room, registry, users)
    }

    /// Check if room exists
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        let (server, _) = self.ctx.scenario().server_and_registry().unwrap();
        server.room_exists(room_key)
    }

    /// Get RoomRef wrapper
    pub fn room(&'_ self, room_key: &RoomKey) -> Option<RoomRef<'_>> {
        let scenario = self.ctx.scenario();
        let (server, registry) = scenario.server_and_registry()?;
        let users = scenario.client_users();
        if server.room_exists(room_key) {
            let room = server.room(room_key);
            Some(RoomRef::new(room, registry, users))
        } else {
            None
        }
    }

    /// Get RoomMut wrapper
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> Option<RoomMut<'_>> {
        let scenario = self.ctx.scenario_mut();
        // Check if room exists before splitting
        let exists = scenario
            .server_and_registry()
            .map(|(server, _)| server.room_exists(room_key))
            .unwrap_or(false);
        if exists {
            let (server, _, registry, users) = scenario.split_for_server_mut();
            let room = server.room_mut(room_key);
            Some(RoomMut::new(room, registry, users))
        } else {
            None
        }
    }

    /// Get all room keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        let (server, _) = self.ctx.scenario().server_and_registry().unwrap();
        server.room_keys()
    }

    /// Get room count
    pub fn rooms_count(&self) -> usize {
        let (server, _) = self.ctx.scenario().server_and_registry().unwrap();
        server.rooms_count()
    }

    // Message Operations

    /// Send message to user
    pub fn send_message<C: Channel, M: Message>(&mut self, client_key: &ClientKey, message: &M) {
        let scenario = self.ctx.scenario_mut();
        if let Some(user_key) = scenario.client_to_user_key(client_key) {
            let (server, _, _, _) = scenario.split_for_server_mut();
            server.send_message::<C, M>(&user_key, message);
        }
    }

    /// Broadcast message to all users
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let (server, _, _, _) = self.ctx.scenario_mut().split_for_server_mut();
        server.broadcast_message::<C, M>(message);
    }

    /// Send request to user
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        client_key: &ClientKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        let scenario = self.ctx.scenario_mut();
        if let Some(user_key) = scenario.client_to_user_key(client_key) {
            let (server, _, _, _) = scenario.split_for_server_mut();
            server.send_request::<C, Q>(&user_key, request)
        } else {
            Err(NaiaServerError::Message("user does not exist".to_string()))
        }
    }

    /// Send response
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        let (server, _, _, _) = self.ctx.scenario_mut().split_for_server_mut();
        server.send_response(response_key, response)
    }

    /// Receive response
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(ClientKey, S)> {
        let scenario = self.ctx.scenario_mut();
        let (server, _, _, users) = scenario.split_for_server_mut();
        server
            .receive_response(response_key)
            .and_then(|(user_key, response)| {
                users
                    .user_to_client_key(&user_key)
                    .map(|client_key| (client_key, response))
            })
    }

    /// Receive tick-buffered messages
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        let (server, _, _, _) = self.ctx.scenario_mut().split_for_server_mut();
        server.receive_tick_buffer_messages(tick)
    }

    /// Generate a new identity token
    ///
    /// This is a thin wrapper around Naia's public API for generating identity tokens.
    /// Useful for creating tokens that can be used in tests, including negative tests
    /// where you want to test with malformed, expired, or reused tokens.
    pub fn generate_identity_token(&self) -> IdentityToken {
        generate_identity_token()
    }

    /// Request authority for an entity on behalf of a client
    ///
    /// This requests authority for the given entity for the specified client.
    /// Returns true if the request was successful, false otherwise.
    pub fn request_authority(&mut self, client_key: &ClientKey, entity: &EntityKey) -> bool {
        let scenario = self.ctx.scenario_mut();
        let Some(user_key) = scenario.client_to_user_key(client_key) else {
            return false;
        };
        let Some(entity) = scenario.entity_registry().server_entity(entity) else {
            return false;
        };
        let (server, _, _, _) = scenario.split_for_server_mut();
        server.client_request_authority(&user_key, &entity);
        true
    }

    /// Release authority for an entity
    ///
    /// This releases authority for the given entity. If a client_key is provided,
    /// authority is released on behalf of that client. If None, authority is released
    /// by the server.
    pub fn release_authority(
        &mut self,
        client_key: Option<&ClientKey>,
        entity: &EntityKey,
    ) -> bool {
        let scenario = self.ctx.scenario_mut();
        let Some(entity) = scenario.entity_registry().server_entity(entity) else {
            return false;
        };
        let user_key = client_key.and_then(|ck| scenario.client_to_user_key(ck));
        let (server, _, _, _) = scenario.split_for_server_mut();
        server.entity_release_authority(user_key.as_ref(), &entity);
        true
    }
}
