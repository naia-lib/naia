// MainServer

use std::{time::Duration};

use naia_bevy_shared::{Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityDoesNotExistError, GlobalEntity, Message, Request, Response, ResponseSendKey, WorldMutType, WorldRefType};
use naia_server::{shared::SocketConfig, transport::Socket, ReplicationConfig, RoomKey, UserKey, Server as NaiaServer, NaiaServerError, Events, EntityOwner, TickBufferMessages};

use crate::{EntityAuthStatus, WorldId, world_entity::WorldEntity, user_scope::{UserScopeMut, UserScopeRef}, user::{UserMut, UserRef}, room::{RoomMut, RoomRef}, Tick, ResponseReceiveKey, Replicate};

pub(crate) struct MainServer {
    server: NaiaServer<WorldEntity>,
}

impl MainServer {

    pub(crate) fn wrap(server: NaiaServer<WorldEntity>) -> Self {
        Self {
            server,
        }
    }

    pub(crate) fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.server.listen(socket);
    }

    pub(crate) fn is_listening(&self) -> bool {
        self.server.is_listening()
    }

    pub(crate) fn socket_config(&self) -> &SocketConfig {
        self.server.socket_config()
    }

    // Connections

    pub(crate) fn accept_connection(&mut self, user_key: &UserKey) {
        self.server.accept_connection(user_key);
    }

    pub(crate) fn reject_connection(&mut self, user_key: &UserKey) {
        self.server.reject_connection(user_key);
    }

    pub(crate) fn receive<W: WorldMutType<WorldEntity>>(&mut self, world: W) -> Events<WorldEntity> {
        self.server.receive(world)
    }

    pub(crate) fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.server.receive_tick_buffer_messages(tick)
    }

    pub(crate) fn send_all_updates<W: WorldRefType<WorldEntity>>(&mut self, world: W) {
        self.server.send_all_updates(world);
    }

    // Messages

    pub(crate) fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        self.server.send_message::<C, M>(user_key, message);
    }

    pub(crate) fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.server.broadcast_message::<C, M>(message);
    }

    pub(crate) fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        self.server.send_request::<C, Q>(user_key, request)
    }

    pub(crate) fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        self.server.send_response(response_key, response)
    }

    pub(crate) fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        self.server.receive_response(response_key)
    }

    // Scopes

    pub(crate) fn scope_checks(&self) -> Vec<(RoomKey, UserKey, WorldEntity)> {
        self.server.scope_checks()
    }

    // Users

    pub(crate) fn user(&self, user_key: &UserKey) -> UserRef {
        UserRef::new(self.server.user(user_key))
    }

    pub(crate) fn user_mut(&mut self, user_key: &UserKey) -> UserMut {
        UserMut::new(self.server.user_mut(user_key))
    }

    pub(crate) fn user_exists(&self, user_key: &UserKey) -> bool {
        self.server.user_exists(user_key)
    }

    pub(crate) fn user_keys(&self) -> Vec<UserKey> {
        self.server.user_keys()
    }

    pub(crate) fn users_count(&self) -> usize {
        self.server.users_count()
    }

    pub(crate) fn user_scope(&self, user_key: &UserKey) -> UserScopeRef {
        UserScopeRef::new(WorldId::main(), self.server.user_scope(user_key))
    }

    pub(crate) fn user_scope_mut(&mut self, user_key: &UserKey) -> UserScopeMut {
        UserScopeMut::new(WorldId::main(), self.server.user_scope_mut(user_key))
    }

    // Rooms

    pub(crate) fn make_room(&mut self) -> RoomMut {
        let room_key = {
            let room_mut = self.server.make_room();
            let room_key = room_mut.key();
            room_key
        };

        RoomMut::new(WorldId::main(), self.server.room_mut(&room_key))
    }

    pub(crate) fn room(&self, room_key: &RoomKey) -> RoomRef {
        RoomRef::new(WorldId::main(), self.server.room(room_key))
    }

    pub(crate) fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut {
        RoomMut::new(WorldId::main(), self.server.room_mut(room_key))
    }

    pub(crate) fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.server.room_exists(room_key)
    }

    pub(crate) fn room_keys(&self) -> Vec<RoomKey> {
        self.server.room_keys()
    }

    pub(crate) fn rooms_count(&self) -> usize {
        self.server.rooms_count()
    }

    // Network Conditions

    pub(crate) fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        self.server.rtt(user_key)
    }

    pub(crate) fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        self.server.jitter(user_key)
    }

    pub(crate) fn current_tick(&self) -> Tick {
        self.server.current_tick()
    }

    pub(crate) fn average_tick_duration(&self) -> Duration {
        self.server.average_tick_duration()
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

    pub(crate) fn entity_owner(&self, world_entity: &WorldEntity) -> EntityOwner {
        self.server.entity_owner(world_entity)
    }

    pub(crate) fn replication_config(&self, world_entity: &WorldEntity) -> Option<ReplicationConfig> {
        self.server.entity_replication_config(world_entity)
    }

    pub(crate) fn configure_entity_replication<W: WorldMutType<WorldEntity>>(&mut self, world: &mut W, world_entity: &WorldEntity, config: ReplicationConfig) {
        self.server.configure_entity_replication(world, world_entity, config);
    }

    pub(crate) fn entity_give_authority(&mut self, user_key: &UserKey, world_entity: &WorldEntity) {
        self.server.entity_give_authority(user_key, world_entity);
    }

    pub(crate) fn entity_take_authority(&mut self, world_entity: &WorldEntity) {
        self.server.entity_take_authority(world_entity);
    }

    pub(crate) fn entity_authority_status(&self, world_entity: &WorldEntity) -> Option<EntityAuthStatus> {
        self.server.entity_authority_status(world_entity)
    }

    // World

    pub(crate) fn despawn_entity_worldless(&mut self, world_entity: &WorldEntity) {
        self.server.despawn_entity_worldless(world_entity);
    }

    pub(crate) fn insert_component_worldless(&mut self, world_entity: &WorldEntity, component: &mut dyn Replicate) {
        self.server.insert_component_worldless(world_entity, component);
    }

    pub(crate) fn remove_component_worldless(&mut self, world_entity: &WorldEntity, component_kind: &ComponentKind) {
        self.server.remove_component_worldless(world_entity, component_kind);
    }
}

impl<'w> EntityAndGlobalEntityConverter<WorldEntity> for MainServer {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<WorldEntity, EntityDoesNotExistError> {
        self.server.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &WorldEntity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.server.entity_to_global_entity(world_entity)
    }
}