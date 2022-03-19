use std::{marker::PhantomData, net::SocketAddr};

use bevy::ecs::{
    entity::Entity,
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{Client as NaiaClient, EntityRef, ProtocolType, Replicate};

use naia_bevy_shared::{WorldProxy, WorldRef};

use super::state::State;

// Client

pub struct Client<'a, P: ProtocolType> {
    world: &'a World,
    client: Mut<'a, NaiaClient<P, Entity>>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Client<'a, P> {
    // Public Methods //

    pub fn new(world: &'a World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<P, Entity>>()
                .expect("Naia Client has not been correctly initialized!");

            Self {
                world,
                client,
                phantom_p: PhantomData,
            }
        }
    }

    //// Connections ////

    pub fn auth<R: Replicate<P>>(&mut self, auth: R) {
        self.client.auth(auth);
    }

    pub fn connect(&mut self, server_session_url: &str) {
        self.client.connect(server_session_url);
    }

    pub fn server_address(&self) -> SocketAddr {
        return self.client.server_address();
    }

    pub fn connected(&self) -> bool {
        return self.client.connected();
    }

    pub fn rtt(&self) -> f32 {
        return self.client.rtt();
    }

    pub fn jitter(&self) -> f32 {
        return self.client.jitter();
    }

    // Interpolation

    pub fn interpolation(&self) -> Option<f32> {
        return self.client.interpolation();
    }

    //// Messages ////
    pub fn send_message<R: Replicate<P>>(&mut self, message_ref: &R, guaranteed_delivery: bool) {
        return self.client.send_message(message_ref, guaranteed_delivery);
    }

    pub fn send_command<R: Replicate<P>>(&mut self, entity: &Entity, command: R) {
        return self.client.send_command(entity, command);
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
        return self.client.client_tick();
    }

    pub fn server_tick(&self) -> Option<u16> {
        return self.client.server_tick();
    }
}

impl<'a, P: ProtocolType> SystemParam for Client<'a, P> {
    type Fetch = State<P>;
}
