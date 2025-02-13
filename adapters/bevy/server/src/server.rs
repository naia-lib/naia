use std::time::Duration;

use bevy_ecs::{
    entity::Entity,
    system::{ResMut, Resource, SystemParam},
    system::Res,
};

use naia_server::{shared::SocketConfig, transport::Socket, EntityOwner, Events, NaiaServerError, ReplicationConfig, RoomKey, Server as NaiaServer, TickBufferMessages, UserKey};

use naia_bevy_shared::{Channel, ComponentKind, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError, GlobalEntity, Message, Request, Response, ResponseReceiveKey, ResponseSendKey, Tick, WorldMutType, WorldRefType};

use crate::{sub_server::SubServer, main_server::MainServer, user_scope::{UserScopeRef, UserScopeMut}, user::{UserMut, UserRef}, room::{RoomRef, RoomMut}, world_entity::{WorldEntity, WorldId}, Replicate};

// ServerWrapper

#[derive(Resource)]
pub(crate) enum ServerWrapper {
    Main(MainServer),
    Sub(SubServer),
}

impl ServerWrapper {
    pub(crate) fn main(server: NaiaServer<WorldEntity>) -> Self {
        Self::Main(MainServer::wrap(server))
    }

    pub(crate) fn sub() -> Self {
        Self::Sub(SubServer::default())
    }

    // Connection

    pub(crate) fn is_listening(&self) -> bool {
        match self {
            ServerWrapper::Main(server) => {
                server.is_listening()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn receive<W: WorldMutType<WorldEntity>>(&mut self, world: W) -> Events<WorldEntity> {
        match self {
            ServerWrapper::Main(server) => {
                server.receive(world)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn send_all_updates<W: WorldRefType<WorldEntity>>(&mut self, world: W) {
        match self {
            ServerWrapper::Main(server) => {
                server.send_all_updates(world);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    // Authority

    pub(crate) fn entity_owner(&self, world_entity: &WorldEntity) -> EntityOwner {
        match self {
            ServerWrapper::Main(server) => {
                server.entity_owner(world_entity)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn entity_authority_status(&self, world_entity: &WorldEntity) -> Option<EntityAuthStatus> {
        match self {
            ServerWrapper::Main(server) => {
                server.entity_authority_status(world_entity)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn configure_entity_replication<W: WorldMutType<WorldEntity>>(
        &mut self,
        world: &mut W,
        world_entity: &WorldEntity,
        config: ReplicationConfig
    ) {
        match self {
            ServerWrapper::Main(server) => {
                server.configure_entity_replication(world, world_entity, config);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    // World

    pub(crate) fn despawn_entity_worldless(&mut self, world_entity: &WorldEntity) {
        match self {
            ServerWrapper::Main(server) => {
                server.despawn_entity_worldless(world_entity);
            }
            ServerWrapper::Sub(server) => {
                server.despawn_entity_worldless(world_entity);
            }
        }
    }

    pub(crate) fn insert_component_worldless(&mut self, world_entity: &WorldEntity, component: &mut dyn Replicate) {
        match self {
            ServerWrapper::Main(server) => {
                server.insert_component_worldless(world_entity, component);
            }
            ServerWrapper::Sub(server) => {
                server.insert_component_worldless(world_entity, component);
            }
        }
    }

    pub(crate) fn remove_component_worldless(&mut self, world_entity: &WorldEntity, component_kind: &ComponentKind) {
        match self {
            ServerWrapper::Main(server) => {
                server.remove_component_worldless(world_entity, component_kind);
            }
            ServerWrapper::Sub(server) => {
                server.remove_component_worldless(world_entity, component_kind);
            }
        }
    }
}

// Server

#[derive(SystemParam)]
pub struct Server<'w> {
    server_wrapper: ResMut<'w, ServerWrapper>,
    world_id: Res<'w, WorldId>,
}

impl<'w> Server<'w> {
    // Public Methods //

    //// Connections ////

    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.listen(socket);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn is_listening(&self) -> bool {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.is_listening()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.accept_connection(user_key);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.reject_connection(user_key);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.socket_config()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Messages ////

    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.send_message::<C, M>(user_key, message);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.broadcast_message::<C, M>(message);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    /// Requests ///
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.send_request::<C, Q>(user_key, request)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.send_response(response_key, response)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.receive_response(response_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.receive_tick_buffer_messages(tick)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server
                    .scope_checks()
                    .iter()
                    .filter(
                        |(_, _, world_entity)| world_entity.world_id().is_main()
                    )
                    .map(
                        |(room_key, user_key, world_entity)|
                        (*room_key, *user_key, world_entity.entity())
                    )
                    .collect()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user_exists(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn user(&self, user_key: &UserKey) -> UserRef {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user_mut(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user_keys()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn users_count(&self) -> usize {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.users_count()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn user_scope(&self, user_key: &UserKey) -> UserScopeRef {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user_scope(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn user_scope_mut(&mut self, user_key: &UserKey) -> UserScopeMut {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.user_scope_mut(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.make_room()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.room_exists(room_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn room(&self, room_key: &RoomKey) -> RoomRef {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.room(room_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.room_mut(room_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.room_keys()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn rooms_count(&self) -> usize {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.rooms_count()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Ticks ////

    pub fn current_tick(&self) -> Tick {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.current_tick()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn average_tick_duration(&self) -> Duration {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.average_tick_duration()
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    //// Network Conditions ////

    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.jitter(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.rtt(user_key)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    // Crate-Public

    pub(crate) fn world_id(&self) -> WorldId {
        *self.world_id
    }

    // Authority

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<ReplicationConfig> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = WorldEntity::new(*self.world_id, *entity);
                server.replication_config(&world_entity)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn entity_give_authority(&mut self, user_key: &UserKey, entity: &Entity) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = WorldEntity::new(*self.world_id, *entity);
                server.entity_give_authority(user_key, &world_entity);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn entity_take_authority(&mut self, entity: &Entity) {
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = WorldEntity::new(*self.world_id, *entity);
                server.entity_take_authority(&world_entity);
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn entity_authority_status(&self, entity: &Entity) -> Option<EntityAuthStatus> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = WorldEntity::new(*self.world_id, *entity);
                server.entity_authority_status(&world_entity)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        let world_entity = WorldEntity::new(*self.world_id, *entity);
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.enable_replication(&world_entity);
            }
            ServerWrapper::Sub(server) => {
                server.enable_replication(&world_entity);
            }
        }
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        let world_entity = WorldEntity::new(*self.world_id, *entity);
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.disable_replication(&world_entity);
            }
            ServerWrapper::Sub(server) => {
                server.disable_replication(&world_entity);
            }
        }
    }

    pub(crate) fn pause_replication(&mut self, entity: &Entity) {
        let world_entity = WorldEntity::new(*self.world_id, *entity);
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.pause_replication(&world_entity);
            }
            ServerWrapper::Sub(server) => {
                server.pause_replication(&world_entity);
            }
        }
    }

    pub(crate) fn resume_replication(&mut self, entity: &Entity) {
        let world_entity = WorldEntity::new(*self.world_id, *entity);
        match &mut *self.server_wrapper {
            ServerWrapper::Main(server) => {
                server.resume_replication(&world_entity);
            }
            ServerWrapper::Sub(server) => {
                server.resume_replication(&world_entity);
            }
        }
    }
}

impl<'w> EntityAndGlobalEntityConverter<Entity> for Server<'w> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = server.global_entity_to_entity(global_entity)?;
                Ok(world_entity.entity())
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        match & *self.server_wrapper {
            ServerWrapper::Main(server) => {
                let world_entity = WorldEntity::main_new(*entity);
                server.entity_to_global_entity(&world_entity)
            }
            ServerWrapper::Sub(_server) => {
                panic!("SubServers do not support this method");
            }
        }
    }
}


