use std::{net::SocketAddr, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    system::{Resource, ResMut, SystemParam},
};

use naia_bevy_shared::{
    Channel, EntityAndGlobalEntityConverter, EntityAuthStatus, EntityDoesNotExistError,
    GlobalEntity, Message, Tick,
};
use naia_client::{shared::SocketConfig, transport::Socket, Client as NaiaClient, NaiaClientError, ConnectionStatus};

use crate::ReplicationConfig;

#[derive(Resource)]
pub struct ClientWrapper<T: Send + Sync + 'static> {
    pub client: NaiaClient<Entity>,
    phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> ClientWrapper<T> {
    pub fn new(client: NaiaClient<Entity>) -> Self {
        Self {
            client,
            phantom_t: PhantomData,
        }
    }
}

// Client
#[derive(SystemParam)]
pub struct Client<'w, T: Send + Sync + 'static> {
    client: ResMut<'w, ClientWrapper<T>>,
}

impl<'w, T: Send + Sync + 'static> Client<'w, T> {
    // Public Methods //

    //// Connections ////

    pub fn auth<M: Message>(&mut self, auth: M) {
        self.client.client.auth(auth);
    }

    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        self.client.client.connect(socket);
    }

    pub fn disconnect(&mut self) {
        self.client.client.disconnect();
    }

    pub fn connection_status(&self) -> ConnectionStatus {
        self.client.client.connection_status()
    }

    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        self.client.client.server_address()
    }

    pub fn rtt(&self) -> f32 {
        self.client.client.rtt()
    }

    pub fn jitter(&self) -> f32 {
        self.client.client.jitter()
    }

    // Config
    pub fn socket_config(&self) -> &SocketConfig {
        self.client.client.socket_config()
    }

    //// Messages ////
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        self.client.client.send_message::<C, M>(message);
    }

    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        self.client.client.send_tick_buffer_message::<C, M>(tick, message);
    }

    //// Ticks ////

    pub fn client_tick(&self) -> Option<Tick> {
        self.client.client.client_tick()
    }

    pub fn server_tick(&self) -> Option<Tick> {
        self.client.client.server_tick()
    }

    // Interpolation

    pub fn client_interpolation(&self) -> Option<f32> {
        self.client.client.client_interpolation()
    }

    pub fn server_interpolation(&self) -> Option<f32> {
        self.client.client.server_interpolation()
    }

    // Entity Registration

    pub(crate) fn enable_replication(&mut self, entity: &Entity) {
        self.client.client.enable_entity_replication(entity);
    }

    pub(crate) fn disable_replication(&mut self, entity: &Entity) {
        self.client.client.disable_entity_replication(entity);
    }

    pub(crate) fn replication_config(&self, entity: &Entity) -> Option<ReplicationConfig> {
        self.client.client.entity_replication_config(entity)
    }

    pub(crate) fn entity_request_authority(&mut self, entity: &Entity) {
        self.client.client.entity_request_authority(entity);
    }

    pub(crate) fn entity_release_authority(&mut self, entity: &Entity) {
        self.client.client.entity_release_authority(entity);
    }

    pub(crate) fn entity_authority_status(&self, entity: &Entity) -> Option<EntityAuthStatus> {
        self.client.client.entity_authority_status(entity)
    }
}

impl<'w, T: Send + Sync + 'static> EntityAndGlobalEntityConverter<Entity> for Client<'w, T> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<Entity, EntityDoesNotExistError> {
        self.client.client.global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        entity: &Entity,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.client.client.entity_to_global_entity(entity)
    }
}
