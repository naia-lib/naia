use std::{time::Duration, collections::HashMap};

use bevy_ecs::{
    entity::Entity,
    system::{ResMut, Resource, SystemParam},
};

use naia_server::{
    shared::SocketConfig, transport::Socket, NaiaServerError, ReplicationConfig, RoomKey, RoomMut,
    RoomRef, Server as NaiaServer, TickBufferMessages, UserKey, UserMut, UserRef, UserScopeMut,
    UserScopeRef,
};

use naia_bevy_shared::{Channel, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError, GlobalEntity, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick, WorldMutType};

use crate::world_entity::{WorldEntity, WorldId};

#[derive(Resource)]
pub(crate) struct ServerWrapper {
    server: NaiaServer<WorldEntity>,
    room_world_ids: HashMap<RoomKey, WorldId>,
}

impl ServerWrapper {

    pub(crate) fn wrap(server: NaiaServer<WorldEntity>) -> Self {
        Self {
            server,
            room_world_ids: HashMap::new(),
        }
    }

    pub(crate) fn is_listening(&self) -> bool {
        self.server.is_listening()
    }

    pub(crate) fn inner(&self) -> &NaiaServer<WorldEntity> {
        &self.server
    }

    pub(crate) fn inner_mut(&mut self) -> &mut NaiaServer<WorldEntity> {
        &mut self.server
    }

    // Entity Replication

    pub(crate) fn enable_replication(&mut self, world_entity: &WorldEntity) {
        self.server.enable_entity_replication(world_entity);
    }

    pub(crate) fn disable_replication(&mut self, world_entity: &WorldEntity) {
        self.server.disable_entity_replication(world_entity);
    }

    pub(crate) fn pause_replication(&mut self, world_entity: &WorldEntity) {
        self.server.pause_entity_replication(world_entity);
    }

    pub(crate) fn resume_replication(&mut self, world_entity: &WorldEntity) {
        self.server.resume_entity_replication(world_entity);
    }

    // Authority related

    pub(crate) fn replication_config(&self, world_entity: &WorldEntity) -> Option<ReplicationConfig> {
        self.server.entity_replication_config(world_entity)
    }

    pub(crate) fn set_replication_config<W: WorldMutType<WorldEntity>>(&mut self, world: &mut W, world_entity: &WorldEntity, config: ReplicationConfig) {
        self.server.configure_entity_replication(world, world_entity, config);
    }

    pub(crate) fn entity_take_authority(&mut self, world_entity: &WorldEntity) {
        self.server.entity_take_authority(world_entity);
    }

    pub(crate) fn entity_authority_status(&self, world_entity: &WorldEntity) -> Option<EntityAuthStatus> {
        self.server.entity_authority_status(world_entity)
    }
}

// Server

#[derive(SystemParam)]
pub struct Server<'w> {
    inner: ResMut<'w, ServerWrapper>,
}

impl<'w> Server<'w> {
    // Public Methods //

    //// Connections ////

    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.inner.server.listen(socket);
    }

    pub fn is_listening(&self) -> bool {
        self.inner.is_listening()
    }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.inner.server.accept_connection(user_key);
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.inner.server.reject_connection(user_key);
    }

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        self.inner.server.socket_config()
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        self.inner.server.send_message::<C, M>(user_key, message)
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.inner.server.broadcast_message::<C, M>(message);
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.inner.server.receive_tick_buffer_messages(tick)
    }

    /// Requests ///
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        self.inner.server.send_request::<C, Q>(user_key, request)
    }

    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.inner.server.send_response(response_key, response)
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        self.inner.server.receive_response(response_key)
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, WorldId, Entity)> {
        self.inner.server.scope_checks().iter().map(|(room_key, user_key, world_entity)| {
            let WorldEntity { world_id, entity } = world_entity;
            (*room_key, *user_key, *world_id, *entity)
        }).collect()
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.inner.server.user_exists(user_key)
    }

    pub fn user(&self, user_key: &UserKey) -> UserRef<WorldEntity> {
        self.inner.server.user(user_key)
    }

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<WorldEntity> {
        self.inner.server.user_mut(user_key)
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        self.inner.server.user_keys()
    }

    pub fn users_count(&self) -> usize {
        self.inner.server.users_count()
    }

    pub fn user_scope(&self, user_key: &UserKey) -> UserScopeRef<Entity> {
        self.inner.server.user_scope(user_key)
    }

    pub fn user_scope_mut(&mut self, user_key: &UserKey) -> UserScopeMut<Entity> {
        self.inner.server.user_scope_mut(user_key)
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut<Entity> {
        self.inner.server.make_room()
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.inner.server.room_exists(room_key)
    }

    pub fn room(&self, room_key: &RoomKey) -> RoomRef<Entity> {
        self.inner.server.room(room_key)
    }

    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<Entity> {
        self.inner.server.room_mut(room_key)
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.inner.server.room_keys()
    }

    pub fn rooms_count(&self) -> usize {
        self.inner.server.rooms_count()
    }

    //// Ticks ////

    pub fn current_tick(&self) -> Tick {
        self.inner.server.current_tick()
    }

    pub fn average_tick_duration(&self) -> Duration {
        self.inner.server.average_tick_duration()
    }

    //// Network Conditions ////

    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        self.inner.server.jitter(user_key)
    }

    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        self.inner.server.rtt(user_key)
    }
}

impl<'w> EntityAndGlobalEntityConverter<WorldEntity> for Server<'w> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<WorldEntity, EntityDoesNotExistError> {
        self.inner.server.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        entity: &WorldEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.inner.server.entity_to_global_entity(entity)
    }
}
