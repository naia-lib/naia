use std::{collections::VecDeque, marker::PhantomData};

use bevy::ecs::{
    system::SystemParam,
    world::{Mut, World},
};

use naia_server::{
    EntityRef, Event, NaiaServerError, ProtocolType, RoomKey, RoomMut, Server as NaiaServer,
    UserKey, UserMut, UserScopeMut,
};

use crate::{
    plugin::resource::ServerResource,
    world::{
        entity::Entity,
        world_proxy::{WorldProxy, WorldRef},
    },
};

use super::{commands::Command, entity_mut::EntityMut, state::State};

// Server

pub struct Server<'a, P: ProtocolType> {
    state: &'a mut State<P>,
    world: &'a World,
    server: Mut<'a, NaiaServer<P, Entity>>,
    resource: Mut<'a, ServerResource>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Server<'a, P> {
    // Public Methods //

    pub fn new(state: &'a mut State<P>, world: &'a World) -> Self {
        unsafe {
            let server = world
                .get_resource_unchecked_mut::<NaiaServer<P, Entity>>()
                .expect("Naia Server has not been correctly initialized!");
            let resource = world
                .get_resource_unchecked_mut::<ServerResource>()
                .expect("Naia Server has not been correctly initialized!");
            Self {
                state,
                world,
                server,
                resource,
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

    //// Entity Scopes ////

    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<P, Entity> {
        return self.server.user_scope(user_key);
    }

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        return self.server.scope_checks();
    }

    //// Users ////

    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<P, Entity> {
        return self.server.user_mut(user_key);
    }

    pub fn users_count(&self) -> usize {
        return self.server.users_count();
    }

    //// Rooms ////

    pub fn make_room(&mut self) -> RoomMut<P, Entity> {
        return self.server.make_room();
    }

    //// Timing ////

    pub fn tick(&mut self) {
        self.resource.ticked = true;
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
