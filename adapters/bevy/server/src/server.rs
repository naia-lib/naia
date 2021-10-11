use std::{collections::VecDeque, marker::PhantomData};

use bevy::ecs::{
    system::SystemParam,
    world::{Mut, World},
};

use naia_server::{
    EntityRef, Event, ImplRef, NaiaServerError, ProtocolType, RoomKey, RoomMut, RoomRef,
    Server as NaiaServer, UserKey, UserMut, UserRef, UserScopeMut,
};

use naia_bevy_shared::{Entity, WorldProxy, WorldRef, tick::Ticker};

use super::{commands::Command, entity_mut::EntityMut, state::State};

// Server

pub struct Server<'a, P: ProtocolType> {
    state: &'a mut State<P>,
    world: &'a World,
    server: Mut<'a, NaiaServer<P, Entity>>,
    ticker: Mut<'a, Ticker>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Server<'a, P> {
    // Public Methods //

    pub fn new(state: &'a mut State<P>, world: &'a World) -> Self {
        unsafe {
            let server = world
                .get_resource_unchecked_mut::<NaiaServer<P, Entity>>()
                .expect("Naia Server has not been correctly initialized!");

            let ticker = world
                .get_resource_unchecked_mut::<Ticker>()
                .expect("Naia Server has not been correctly initialized!");

            Self {
                state,
                world,
                server,
                ticker,
                phantom_p: PhantomData,
            }
        }
    }

    pub fn receive(&mut self) -> VecDeque<Result<Event<P, Entity>, NaiaServerError>> {
        return self.server.receive(self.world.proxy());
    }

    //// Connections ////

    pub fn accept_connection(&mut self, user_key: &UserKey) {
        self.server.accept_connection(user_key);
    }

    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.server.reject_connection(user_key);
    }

    //// Messages ////
    pub fn queue_message<R: ImplRef<P>>(
        &mut self,
        user_key: &UserKey,
        message_ref: &R,
        guaranteed_delivery: bool,
    ) {
        return self
            .server
            .queue_message(user_key, message_ref, guaranteed_delivery);
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        return self.server.scope_checks();
    }

    pub fn send_all_updates(&mut self) {
        return self.server.send_all_updates(self.world.proxy());
    }

    //// Entities ////

    pub fn spawn(&mut self) -> EntityMut<'a, '_, P> {
        let entity = Entity::new(self.world.entities().reserve_entity());
        self.server.spawn_entity_at(&entity);
        EntityMut::new(entity, self)
    }

    pub fn entity(&self, entity: &Entity) -> EntityRef<P, Entity, WorldRef> {
        return self.server.entity(self.world.proxy(), entity);
    }

    pub fn entity_mut(&mut self, entity: &Entity) -> EntityMut<'a, '_, P> {
        EntityMut::new(*entity, self)
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.server.entities(&self.world.proxy());
    }

    //// Users ////

    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        return self.server.user_exists(user_key);
    }

    pub fn user(&self, user_key: &UserKey) -> UserRef<P, Entity> {
        return self.server.user(user_key);
    }

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, Entity> {
        return self.server.user_mut(user_key);
    }

    pub fn user_keys(&self) -> Vec<UserKey> {
        return self.server.user_keys();
    }

    pub fn users_count(&self) -> usize {
        return self.server.users_count();
    }

    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, Entity> {
        return self.server.user_scope(user_key);
    }

    pub fn user_scope_has_entity(&self, user_key: &UserKey, entity: &Entity) -> bool {
        return self.server.user_scope_has_entity(user_key, entity);
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut<P, Entity> {
        return self.server.make_room();
    }

    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        return self.server.room_exists(room_key);
    }

    pub fn room(&self, room_key: &RoomKey) -> RoomRef<P, Entity> {
        return self.server.room(room_key);
    }

    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<P, Entity> {
        return self.server.room_mut(room_key);
    }

    pub fn room_keys(&self) -> Vec<RoomKey> {
        return self.server.room_keys();
    }

    pub fn rooms_count(&self) -> usize {
        return self.server.rooms_count();
    }

    //// Ticks ////

    pub fn client_tick(&self, user_key: &UserKey) -> Option<u16> {
        return self.server.client_tick(user_key);
    }

    pub fn server_tick(&self) -> u16 {
        return self.server.server_tick();
    }

    pub fn tick_start(&mut self) {
        self.ticker.tick_start();
    }

    // Crate-public methods

    pub(crate) fn add<C: Command<P>>(&mut self, command: C) {
        self.state.push(command);
    }

    // users

    pub(crate) fn entity_disown(&mut self, entity: &Entity) {
        self.server.worldless_entity_mut(entity).disown();
    }

    // rooms

    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).add_entity(entity);
    }

    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity: &Entity) {
        self.server.room_mut(room_key).remove_entity(entity);
    }

    // Private methods
}

impl<'a, P: ProtocolType> SystemParam for Server<'a, P> {
    type Fetch = State<P>;
}
