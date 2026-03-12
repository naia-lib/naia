use std::time::Duration;

use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    system::{ResMut, SystemParam},
    world::{Mut, World},
};

use naia_server::{
    shared::SocketConfig, transport::Socket, EntityOwner, Events, NaiaServerError,
    ReplicationConfig, RoomKey, RoomMut, RoomRef, Server as NaiaServer, TickBufferMessages,
    TickEvents, UserKey, UserMut, UserRef, UserScopeMut, UserScopeRef,
    WorldServer as NaiaWorldServer, WorldServer,
};

use naia_bevy_shared::{
    Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, GlobalEntity, Instant, Message, Request, Response, ResponseReceiveKey,
    ResponseSendKey, Tick, WorldMutType, WorldRefType,
};

use crate::Replicate;

#[derive(Resource)]
pub(crate) enum ServerImpl {
    Full(NaiaServer<Entity>),
    WorldOnly(NaiaWorldServer<Entity>),
}

impl ServerImpl {
    pub(crate) fn full(full_server: NaiaServer<Entity>) -> Self {
        Self::Full(full_server)
    }

    pub(crate) fn world_only(world_only_server: NaiaWorldServer<Entity>) -> Self {
        Self::WorldOnly(world_only_server)
    }

    pub(crate) fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        match self {
            Self::Full(server) => server.listen(socket),
            Self::WorldOnly(server) => {
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (_auth_sender, _auth_receiver, packet_sender, packet_receiver) =
                    boxed_socket.listen();
                server.io_load(packet_sender, packet_receiver);
            }
        }
    }

    pub(crate) fn is_listening(&self) -> bool {
        match self {
            Self::Full(server) => server.is_listening(),
            Self::WorldOnly(server) => server.is_listening(),
        }
    }

    pub(crate) fn receive_all_packets(&mut self) {
        match self {
            Self::Full(server) => server.receive_all_packets(),
            Self::WorldOnly(server) => server.receive_all_packets(),
        }
    }

    pub(crate) fn process_all_packets<W: WorldMutType<Entity>>(&mut self, world: W, now: &Instant) {
        match self {
            Self::Full(server) => server.process_all_packets(world, now),
            Self::WorldOnly(server) => server.process_all_packets(world, now),
        }
    }

    pub(crate) fn take_world_events(&mut self) -> Events<Entity> {
        match self {
            Self::Full(server) => server.take_world_events(),
            Self::WorldOnly(server) => {
                let world_events = server.take_world_events();
                Events::<Entity>::from(world_events)
            }
        }
    }

    pub(crate) fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        match self {
            Self::Full(server) => server.take_tick_events(now),
            Self::WorldOnly(server) => server.take_tick_events(now),
        }
    }

    pub(crate) fn send_all_packets<W: WorldRefType<Entity>>(&mut self, world: W) {
        match self {
            Self::Full(server) => server.send_all_packets(world),
            Self::WorldOnly(server) => server.send_all_packets(world),
        }
    }

    pub(crate) fn entity_authority_status<W: WorldRefType<Entity>>(
        &self,
        world: W,
        entity: &Entity,
    ) -> Option<EntityAuthStatus> {
        if !world.has_entity(entity) {
            return None;
        }
        match self {
            Self::Full(server) => server.entity(world, entity).authority(),
            Self::WorldOnly(server) => server.entity(world, entity).authority(),
        }
    }

    pub(crate) fn entity_owner<W: WorldRefType<Entity>>(
        &self,
        world: W,
        entity: &Entity,
    ) -> EntityOwner {
        match self {
            Self::Full(server) => server.entity(world, entity).owner(),
            Self::WorldOnly(server) => server.entity(world, entity).owner(),
        }
    }

    pub(crate) fn configure_entity_replication<W: WorldMutType<Entity>>(
        &mut self,
        world: &mut W,
        world_entity: &Entity,
        config: ReplicationConfig,
    ) {
        match self {
            Self::Full(server) => {
                server.configure_entity_replication::<W>(world, world_entity, config)
            }
            Self::WorldOnly(server) => {
                server.configure_entity_replication::<W>(world, world_entity, config)
            }
        }
    }

    pub(crate) fn insert_component_worldless(
        &mut self,
        entity: &Entity,
        component: &mut dyn Replicate,
    ) {
        match self {
            Self::Full(server) => server.insert_component_worldless(entity, component),
            Self::WorldOnly(server) => server.insert_component_worldless(entity, component),
        }
    }

    pub(crate) fn remove_component_worldless(
        &mut self,
        entity: &Entity,
        component_kind: &ComponentKind,
    ) {
        match self {
            Self::Full(server) => server.remove_component_worldless(entity, component_kind),
            Self::WorldOnly(server) => server.remove_component_worldless(entity, component_kind),
        }
    }

    pub(crate) fn despawn_entity_worldless(&mut self, entity: &Entity) {
        match self {
            Self::Full(server) => server.despawn_entity_worldless(entity),
            Self::WorldOnly(server) => server.despawn_entity_worldless(entity),
        }
    }
}

// Server

#[derive(SystemParam)]
pub struct Server<'w> {
    server_impl: ResMut<'w, ServerImpl>,
}

impl<'w> Server<'w> {
    // Public Methods //

    //// Connections ////

    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.server_impl.listen(socket);
    }

    pub fn is_listening(&self) -> bool {
        self.server_impl.is_listening()
    }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.accept_connection(user_key),
        }
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.reject_connection(user_key),
        }
    }

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        match &*self.server_impl {
            ServerImpl::WorldOnly(_server) => {
                panic!("WorldOnly Servers do not support this function")
            }
            ServerImpl::Full(server) => server.socket_config(),
        }
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_message::<C, M>(user_key, message),
            ServerImpl::Full(server) => server.send_message::<C, M>(user_key, message),
        }
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.broadcast_message::<C, M>(message),
            ServerImpl::Full(server) => server.broadcast_message::<C, M>(message),
        }
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.receive_tick_buffer_messages(tick),
            ServerImpl::Full(server) => server.receive_tick_buffer_messages(tick),
        }
    }

    /// Requests ///
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_request::<C, Q>(user_key, request),
            ServerImpl::Full(server) => server.send_request::<C, Q>(user_key, request),
        }
    }

    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.send_response(response_key, response),
            ServerImpl::Full(server) => server.send_response(response_key, response),
        }
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.receive_response(response_key),
            ServerImpl::Full(server) => server.receive_response(response_key),
        }
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.scope_checks(),
            ServerImpl::Full(server) => server.scope_checks(),
        }
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_exists(user_key),
            ServerImpl::Full(server) => server.user_exists(user_key),
        }
    }

    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user(user_key),
            ServerImpl::Full(server) => server.user(user_key),
        }
    }

    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_mut(user_key),
            ServerImpl::Full(server) => server.user_mut(user_key),
        }
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_keys(),
            ServerImpl::Full(server) => server.user_keys(),
        }
    }

    pub fn users_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.users_count(),
            ServerImpl::Full(server) => server.users_count(),
        }
    }

    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_scope(user_key),
            ServerImpl::Full(server) => server.user_scope(user_key),
        }
    }

    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.user_scope_mut(user_key),
            ServerImpl::Full(server) => server.user_scope_mut(user_key),
        }
    }

    //// Rooms ////

    pub fn make_room(&'_ mut self) -> RoomMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.make_room(),
            ServerImpl::Full(server) => server.make_room(),
        }
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_exists(room_key),
            ServerImpl::Full(server) => server.room_exists(room_key),
        }
    }

    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, Entity> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room(room_key),
            ServerImpl::Full(server) => server.room(room_key),
        }
    }

    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, Entity> {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_mut(room_key),
            ServerImpl::Full(server) => server.room_mut(room_key),
        }
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.room_keys(),
            ServerImpl::Full(server) => server.room_keys(),
        }
    }

    pub fn rooms_count(&self) -> usize {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.rooms_count(),
            ServerImpl::Full(server) => server.rooms_count(),
        }
    }

    //// Ticks ////

    pub fn current_tick(&self) -> Tick {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.current_tick(),
            ServerImpl::Full(server) => server.current_tick(),
        }
    }

    pub fn average_tick_duration(&self) -> Duration {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.average_tick_duration(),
            ServerImpl::Full(server) => server.average_tick_duration(),
        }
    }

    //// Network Conditions ////

    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.jitter(user_key),
            ServerImpl::Full(server) => server.jitter(user_key),
        }
    }

    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.rtt(user_key),
            ServerImpl::Full(server) => server.rtt(user_key),
        }
    }

    // Entity Replication

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.enable_entity_replication(entity),
            ServerImpl::Full(server) => server.enable_entity_replication(entity),
        }
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.disable_entity_replication(entity),
            ServerImpl::Full(server) => server.disable_entity_replication(entity),
        }
    }

    pub(crate) fn pause_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.pause_entity_replication(entity),
            ServerImpl::Full(server) => server.pause_entity_replication(entity),
        }
    }

    pub(crate) fn resume_replication(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => server.resume_entity_replication(entity),
            ServerImpl::Full(server) => server.resume_entity_replication(entity),
        }
    }

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<ReplicationConfig> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.entity_replication_config(entity),
            ServerImpl::Full(server) => server.entity_replication_config(entity),
        }
    }

    pub(crate) fn entity_take_authority(&mut self, entity: &Entity) {
        match &mut *self.server_impl {
            ServerImpl::WorldOnly(server) => {
                let _ = server.entity_take_authority(entity);
            }
            ServerImpl::Full(server) => {
                let _ = server.entity_take_authority(entity);
            }
        }
    }

    pub(crate) fn entity_authority_status(&self, _entity: &Entity) -> Option<EntityAuthStatus> {
        todo!("entity_authority_status requires world access; use ServerImpl directly in exclusive systems")
    }

    pub fn world_only_resource_scope(
        world: &mut World,
        f: impl FnOnce(&mut World, &mut WorldServer<Entity>),
    ) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| match &mut *server {
            ServerImpl::WorldOnly(server) => {
                f(world, server);
            }
            ServerImpl::Full(_) => {
                panic!("Expected WorldOnly Server, found Full Server");
            }
        })
    }
}

impl<'w> EntityAndGlobalEntityConverter<Entity> for Server<'w> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.global_entity_to_entity(global_entity),
            ServerImpl::Full(server) => server.global_entity_to_entity(global_entity),
        }
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match &*self.server_impl {
            ServerImpl::WorldOnly(server) => server.entity_to_global_entity(entity),
            ServerImpl::Full(server) => server.entity_to_global_entity(entity),
        }
    }
}
