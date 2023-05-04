use std::net::SocketAddr;

use bevy_ecs::{
    entity::Entity,
    system::{ResMut, SystemParam},
};

use naia_bevy_shared::{Channel, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError, GlobalEntity, Message, Tick};
use naia_client::{shared::SocketConfig, transport::Socket, Client as NaiaClient, NaiaClientError};

use crate::ReplicationConfig;

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

    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.client.connect(socket);
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

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        self.client.socket_config()
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

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        self.client.enable_entity_replication(entity);
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        self.client.disable_entity_replication(entity);
    }

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<ReplicationConfig> {
        self.client.entity_replication_config(entity)
    }

    pub(crate) fn entity_request_authority(&mut self, entity: &Entity) {
        self.client.entity_request_authority(entity);
    }

    pub(crate) fn entity_release_authority(&mut self, entity: &Entity) {
        self.client.entity_release_authority(entity);
    }

    pub(crate) fn entity_authority_status(&self, entity: &Entity) -> EntityAuthStatus {
        self.client.entity_authority_status(entity)
    }
}

impl<'w> EntityAndGlobalEntityConverter<Entity> for Client<'w> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        self.client.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.client.entity_to_global_entity(entity)
    }
}
