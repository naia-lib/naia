use std::{marker::PhantomData, net::SocketAddr};

use bevy_ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{
    shared::{ChannelIndex, Protocolize, ReplicateSafe},
    Client as NaiaClient, EntityRef,
};

use naia_bevy_shared::{WorldProxy, WorldRef};
use naia_client::shared::{EntityHandle, EntityHandleConverter};

use super::state::State;

// Client

pub struct Client<'a, P: Protocolize, C: ChannelIndex> {
    world: &'a World,
    client: Mut<'a, NaiaClient<P, Entity, C>>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: Protocolize, C: ChannelIndex> Client<'a, P, C> {
    // Public Methods //

    pub fn new(world: &'a World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<P, Entity, C>>()
                .expect("Naia Client has not been correctly initialized!");

            Self {
                world,
                client,
                phantom_p: PhantomData,
            }
        }
    }

    //// Connections ////

    pub fn auth<R: ReplicateSafe<P>>(&mut self, auth: R) {
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

    pub fn server_address(&self) -> SocketAddr {
        self.client.server_address()
    }

    pub fn rtt(&self) -> f32 {
        self.client.rtt()
    }

    pub fn jitter(&self) -> f32 {
        self.client.jitter()
    }

    // Interpolation

    pub fn interpolation(&self) -> Option<f32> {
        self.client.interpolation()
    }

    //// Messages ////
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, channel: C, message: &R) {
        self.client.send_message(channel, message)
    }

    //// Entities ////

    pub fn entity(&self, entity: &Entity) -> EntityRef<P, Entity, WorldRef> {
        return self.client.entity(self.world.proxy(), entity);
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.client.entities(&self.world.proxy());
    }

    //// Ticks ////

    pub fn client_tick(&self) -> Option<u16> {
        self.client.client_tick()
    }
}

impl<'a, P: Protocolize, C: ChannelIndex> SystemParam for Client<'a, P, C> {
    type Fetch = State<P, C>;
}

impl<'a, P: Protocolize, C: ChannelIndex> EntityHandleConverter<Entity> for Client<'a, P, C> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Entity {
        self.client.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &Entity) -> EntityHandle {
        self.client.entity_to_handle(entity)
    }
}
