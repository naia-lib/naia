use std::net::SocketAddr;

use bevy_ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{Client as NaiaClient, EntityRef, NaiaClientError};

use naia_bevy_shared::{
    Channel, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, Tick,
    WorldProxy, WorldRef, WorldRefType,
};

use crate::{commands::Command, entity_mut::EntityMut, state::State};

// Client

pub struct Client<'world, 'state> {
    state: &'state mut State,
    world: &'world World,
    client: Mut<'world, NaiaClient<Entity>>,
}

impl<'world, 'state> Client<'world, 'state> {
    // Public Methods //

    pub fn new(state: &'state mut State, world: &'world World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<Entity>>()
                .expect("Naia Client has not been correctly initialized!");

            Self {
                state,
                world,
                client,
            }
        }
    }

    //// Connections ////

    pub fn auth<M: Message>(&mut self, auth: M) {
        self.client.auth(auth);
    }

    pub fn connect(&mut self, server_address: &str) {
        self.client.connect(server_address);
    }

    pub fn disconnect(&mut self) {
        self.client.disconnect();
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    pub fn is_connecting(&self) -> bool {
        self.client.is_connecting()
    }

    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        self.client.server_address()
    }

    pub fn rtt(&self) -> f32 {
        self.client.rtt()
    }

    pub fn jitter(&self) -> f32 {
        self.client.jitter()
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.client.send_message::<C, M>(message);
    }

    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        self.client.send_tick_buffer_message::<C, M>(tick, message);
    }

    //// Entities ////

    pub fn spawn<'a>(&'a mut self) -> EntityMut<'a, 'world, 'state> {
        let entity = self.world.entities().reserve_entity();
        self.client.spawn_entity_at(&entity);
        EntityMut::new(entity, self)
    }

    pub fn has_entity(&self, entity: &Entity) -> bool {
        self.world.proxy().has_entity(entity)
    }

    pub fn entity(&self, entity: &Entity) -> EntityRef<Entity, WorldRef> {
        return self.client.entity(self.world.proxy(), entity);
    }

    pub fn entity_mut<'a>(&'a mut self, entity: &Entity) -> EntityMut<'a, 'world, 'state> {
        EntityMut::new(*entity, self)
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.client.entities(&self.world.proxy());
    }

    //// Ticks ////

    pub fn client_tick(&self) -> Option<Tick> {
        self.client.client_tick()
    }

    pub fn server_tick(&self) -> Option<Tick> {
        self.client.server_tick()
    }

    // Interpolation

    pub fn client_interpolation(&self) -> Option<f32> {
        self.client.client_interpolation()
    }

    pub fn server_interpolation(&self) -> Option<f32> {
        self.client.server_interpolation()
    }

    // Crate-public methods

    pub(crate) fn queue_command<COMMAND: Command>(&mut self, command: COMMAND) {
        self.state.push(command);
    }
}

impl<'world, 'state> SystemParam for Client<'world, 'state> {
    type Fetch = State;
}

impl<'world, 'state> EntityHandleConverter<Entity> for Client<'world, 'state> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.client.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.client.entity_to_handle(entity)
    }
}
