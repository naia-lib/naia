use std::net::SocketAddr;

use bevy_ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{Client as NaiaClient, EntityRef, NaiaClientError};

use naia_bevy_shared::{
    Channel, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, Tick,
    WorldProxy, WorldRef,
};

use super::state::State;

// Client

pub struct Client<'a> {
    world: &'a World,
    client: Mut<'a, NaiaClient<Entity>>,
}

impl<'a> Client<'a> {
    // Public Methods //

    pub fn new(world: &'a World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<Entity>>()
                .expect("Naia Client has not been correctly initialized!");

            Self { world, client }
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

    // Interpolation

    pub fn client_interpolation(&self) -> Option<f32> {
        self.client.client_interpolation()
    }

    pub fn server_interpolation(&self) -> Option<f32> {
        self.client.server_interpolation()
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.client.send_message::<C, M>(message);
    }

    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        self.client.send_tick_buffer_message::<C, M>(tick, message);
    }

    //// Entities ////

    pub fn entity(&self, entity: &Entity) -> EntityRef<Entity, WorldRef> {
        return self.client.entity(self.world.proxy(), entity);
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
}

impl<'a> SystemParam for Client<'a> {
    type Fetch = State;
}

impl<'a> EntityHandleConverter<Entity> for Client<'a> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.client.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.client.entity_to_handle(entity)
    }
}
