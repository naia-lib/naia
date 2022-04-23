use std::{collections::VecDeque, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_server::{
    shared::{ChannelIndex, EntityHandleConverter, Protocolize, ReplicateSafe},
    EntityRef, Event, NaiaServerError, RoomKey, RoomMut, RoomRef, Server as NaiaServer,
    ServerAddrs, UserKey, UserMut, UserRef, UserScopeMut,
};

use crate::shared::EntityHandle;
use naia_bevy_shared::{WorldProxy, WorldRef};

use super::{commands::Command, entity_mut::EntityMut, state::State};

// Server

pub struct Server<'world, 'state, P: Protocolize, C: ChannelIndex> {
    state: &'state mut State<P, C>,
    world: &'world World,
    server: Mut<'world, NaiaServer<P, Entity, C>>,
    phantom_p: PhantomData<P>,
}

impl<'world, 'state, P: Protocolize, C: ChannelIndex> Server<'world, 'state, P, C> {
    // Public Methods //

    pub fn new(state: &'state mut State<P, C>, world: &'world World) -> Self {
        unsafe {
            let server = world
                .get_resource_unchecked_mut::<NaiaServer<P, Entity, C>>()
                .expect("Naia Server has not been correctly initialized!");

            Self {
                state,
                world,
                server,
                phantom_p: PhantomData,
            }
        }
    }

    pub fn receive(&mut self) -> VecDeque<Result<Event<P, C>, NaiaServerError>> {
        self.server.receive()
    }

    //// Connections ////

    pub fn listen(&mut self, server_addrs: &ServerAddrs) {
        self.server.listen(server_addrs);
    }

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.server.accept_connection(user_key);
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.server.reject_connection(user_key);
    }

    //// Messages ////
    pub fn send_message<R: ReplicateSafe<P>>(
        &mut self,
        user_key: &UserKey,
        channel: C,
        message: &R,
    ) {
        self.server.send_message(user_key, channel, message)
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        self.server.scope_checks()
    }

    pub fn send_all_updates(&mut self) {
        return self.server.send_all_updates(self.world.proxy());
    }

    //// Entities ////

    pub fn spawn<'a>(&'a mut self) -> EntityMut<'a, 'world, 'state, P, C> {
        let entity = self.world.entities().reserve_entity();
        self.server.spawn_entity_at(&entity);
        EntityMut::new(entity, self)
    }

    pub fn entity(&self, entity: &Entity) -> EntityRef<P, Entity, WorldRef> {
        return self.server.entity(self.world.proxy(), entity);
    }

    pub fn entity_mut<'a>(&'a mut self, entity: &Entity) -> EntityMut<'a, 'world, 'state, P, C> {
        EntityMut::new(*entity, self)
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.server.entities(self.world.proxy());
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.server.user_exists(user_key)
    }

    pub fn user(&self, user_key: &UserKey) -> UserRef<P, Entity, C> {
        self.server.user(user_key)
    }

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, Entity, C> {
        self.server.user_mut(user_key)
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        self.server.user_keys()
    }

    pub fn users_count(&self) -> usize {
        self.server.users_count()
    }

    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, Entity, C> {
        self.server.user_scope(user_key)
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut<P, Entity, C> {
        self.server.make_room()
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.server.room_exists(room_key)
    }

    pub fn room(&self, room_key: &RoomKey) -> RoomRef<P, Entity, C> {
        self.server.room(room_key)
    }

    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<P, Entity, C> {
        self.server.room_mut(room_key)
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.server.room_keys()
    }

    pub fn rooms_count(&self) -> usize {
        self.server.rooms_count()
    }

    //// Ticks ////

    pub fn client_tick(&self, user_key: &UserKey) -> Option<u16> {
        self.server.client_tick(user_key)
    }

    pub fn server_tick(&self) -> Option<u16> {
        self.server.server_tick()
    }

    // Crate-public methods

    pub(crate) fn queue_command<COMMAND: Command<P, C>>(&mut self, command: COMMAND) {
        self.state.push(command);
    }

    // rooms

    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).add_entity(entity);
    }

    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).remove_entity(entity);
    }
}

impl<'world, 'state, P: Protocolize, C: ChannelIndex> SystemParam for Server<'world, 'state, P, C> {
    type Fetch = State<P, C>;
}

impl<'world, 'state, P: Protocolize, C: ChannelIndex> EntityHandleConverter<Entity>
    for Server<'world, 'state, P, C>
{
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.server.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> EntityHandle {
        self.server.entity_to_handle(entity)
    }
}
