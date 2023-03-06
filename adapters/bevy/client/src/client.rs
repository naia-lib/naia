use std::net::SocketAddr;

use bevy_ecs::{
    entity::Entity,
    system::{ResMut, SystemParam},
};

use naia_client::{Client as NaiaClient, NaiaClientError};

use naia_bevy_shared::{
    Channel, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, Tick,
};

// Client
#[derive(SystemParam)]
pub struct Client<'w> {
    client: ResMut<'w, NaiaClient<Entity>>,
}

impl<'w> Client<'w> {
    // Public Methods //

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

    // Entity Registration

    pub fn enable_replication(&mut self, entity: &Entity) {
        self.client.enable_replication(entity);
    }

    pub fn disable_replication(&mut self, entity: &Entity) {
        self.client.disable_replication(entity);
    }
}

impl<'w> EntityHandleConverter<Entity> for Client<'w> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.client.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.client.entity_to_handle(entity)
    }
}
