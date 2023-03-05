use std::marker::PhantomData;
use std::time::Duration;

use bevy_ecs::{
    entity::Entity,
    system::{SystemParam, ReadOnlySystemParam, ResMut, Deferred, Res},
    world::World,
};

use naia_server::{
    EntityRef, RoomKey, RoomMut, RoomRef, Server as NaiaServer, ServerAddrs, TickBufferMessages,
    UserKey, UserMut, UserRef, UserScopeMut,
};

use naia_bevy_shared::{
    Channel, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, Tick,
    WorldProxy, WorldRef, WorldRefType,
};

// Server

#[derive(SystemParam)]
pub struct Server<'w> {
    server: ResMut<'w, NaiaServer<Entity>>,
}

impl<'w> Server<'w> {
    // Public Methods //

    //// Connections ////

    pub fn listen(&mut self, server_addrs: &ServerAddrs) {
        self.server.listen(server_addrs);
    }

    pub fn is_listening(&self) -> bool { self.server.is_listening() }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.server.accept_connection(user_key);
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.server.reject_connection(user_key);
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        self.server.send_message::<C, M>(user_key, message)
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.server.broadcast_message::<C, M>(message);
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        self.server.receive_tick_buffer_messages(tick)
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        self.server.scope_checks()
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.server.user_exists(user_key)
    }

    pub fn user(&self, user_key: &UserKey) -> UserRef<Entity> {
        self.server.user(user_key)
    }

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<Entity> {
        self.server.user_mut(user_key)
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        self.server.user_keys()
    }

    pub fn users_count(&self) -> usize {
        self.server.users_count()
    }

    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<Entity> {
        self.server.user_scope(user_key)
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut<Entity> {
        self.server.make_room()
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.server.room_exists(room_key)
    }

    pub fn room(&self, room_key: &RoomKey) -> RoomRef<Entity> {
        self.server.room(room_key)
    }

    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<Entity> {
        self.server.room_mut(room_key)
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.server.room_keys()
    }

    pub fn rooms_count(&self) -> usize {
        self.server.rooms_count()
    }

    //// Ticks ////

    pub fn current_tick(&self) -> Tick {
        self.server.current_tick()
    }

    pub fn average_tick_duration(&self) -> Duration {
        self.server.average_tick_duration()
    }

    // Crate-public methods

    // rooms

    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).add_entity(entity);
    }

    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).remove_entity(entity);
    }
}

impl<'w> EntityHandleConverter<Entity> for Server<'w> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.server.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.server.entity_to_handle(entity)
    }
}
