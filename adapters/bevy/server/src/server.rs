use bevy_ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_server::{
    EntityRef, RoomKey, RoomMut, RoomRef, Server as NaiaServer, ServerAddrs, UserKey, UserMut,
    UserRef, UserScopeMut,
};

use naia_bevy_shared::{
    Channel, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, Tick,
    WorldProxy, WorldRef, WorldRefType,
};

use super::{commands::Command, entity_mut::EntityMut, state::State};

// Server

pub struct Server<'world, 'state> {
    state: &'state mut State,
    world: &'world World,
    server: Mut<'world, NaiaServer<Entity>>,
}

impl<'world, 'state> Server<'world, 'state> {
    // Public Methods //

    pub fn new(state: &'state mut State, world: &'world World) -> Self {
        unsafe {
            let server = world
                .get_resource_unchecked_mut::<NaiaServer<Entity>>()
                .expect("Naia Server has not been correctly initialized!");

            Self {
                state,
                world,
                server,
            }
        }
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
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        self.server.send_message::<C, M>(user_key, message)
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.server.broadcast_message::<C, M>(message);
    }

    //// Updates ////

    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, Entity)> {
        self.server.scope_checks()
    }

    pub fn send_all_updates(&mut self) {
        return self.server.send_all_updates(self.world.proxy());
    }

    //// Entities ////

    pub fn spawn<'a>(&'a mut self) -> EntityMut<'a, 'world, 'state> {
        let entity = self.world.entities().reserve_entity();
        self.server.spawn_entity_at(&entity);
        EntityMut::new(entity, self)
    }

    /// Returns true if the server's [`WorldProxy`] has the entity
    pub fn has_entity(&self, entity: &Entity) -> bool {
        self.world.proxy().has_entity(entity)
    }

    pub fn entity(&self, entity: &Entity) -> EntityRef<Entity, WorldRef> {
        return self.server.entity(self.world.proxy(), entity);
    }

    pub fn entity_mut<'a>(&'a mut self, entity: &Entity) -> EntityMut<'a, 'world, 'state> {
        EntityMut::new(*entity, self)
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.server.entities(self.world.proxy());
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

    pub fn client_tick(&self, user_key: &UserKey) -> Option<Tick> {
        self.server.client_tick(user_key)
    }

    pub fn server_tick(&self) -> Option<Tick> {
        self.server.server_tick()
    }

    // Crate-public methods

    pub(crate) fn queue_command<COMMAND: Command>(&mut self, command: COMMAND) {
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

impl<'world, 'state> SystemParam for Server<'world, 'state> {
    type Fetch = State;
}

impl<'world, 'state> EntityHandleConverter<Entity> for Server<'world, 'state> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.server.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.server.entity_to_handle(entity)
    }
}
