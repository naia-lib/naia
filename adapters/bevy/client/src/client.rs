use std::{collections::VecDeque, marker::PhantomData, net::SocketAddr};

use bevy::ecs::{
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{Client as NaiaClient, EntityRef, Event, ImplRef, NaiaClientError, ProtocolType};

use naia_bevy_shared::{Entity, WorldProxy, WorldProxyMut, WorldRef};

use super::{state::State, resource::ClientResource};

// Client

pub struct Client<'a, P: ProtocolType> {
    world: &'a World,
    client: Mut<'a, NaiaClient<P, Entity>>,
    resource: Mut<'a, ClientResource>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Client<'a, P> {
    // Public Methods //

    pub fn new(world: &'a World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<P, Entity>>()
                .expect("Naia Client has not been correctly initialized!");

            let resource = world
                .get_resource_unchecked_mut::<ClientResource>()
                .expect("Naia Client has not been correctly initialized!");

            Self {
                world,
                client,
                resource,
                phantom_p: PhantomData,
            }
        }
    }

    //// Connections ////

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

    pub fn receive(&mut self) -> VecDeque<Result<Event<P, Entity>, NaiaClientError>> {
        return self.client.receive(&mut self.world.proxy_mut());
    }

    // Interpolation

    pub fn interpolation(&self) -> f32 {
        return self.client.interpolation();
    }

    //// Messages ////
    pub fn queue_message<R: ImplRef<P>>(&mut self, message_ref: &R, guaranteed_delivery: bool) {
        return self.client.queue_message(message_ref, guaranteed_delivery);
    }

    pub fn queue_command<R: ImplRef<P>>(&mut self, entity: &Entity, command_ref: &R) {
        return self.client.queue_command(entity, command_ref);
    }

    //// Entities ////

    pub fn entity(&self, entity: &Entity) -> EntityRef<P, Entity, WorldRef> {
        return self.client.entity(self.world.proxy(), entity);
    }

    pub fn entities(&self) -> Vec<Entity> {
        return self.client.entities(&self.world.proxy());
    }

    //// Ticks ////

    pub fn client_tick(&self) -> u16 {
        return self.client.client_tick();
    }

    pub fn server_tick(&self) -> u16 {
        return self.client.server_tick();
    }

    pub fn tick_start(&mut self) {
        self.resource.ticked = true;
    }

    pub fn tick_finish(&mut self) {
        self.resource.ticked = false;
    }

    pub fn has_ticked(&self) -> bool {
        return self.resource.ticked;
    }
}

impl<'a, P: ProtocolType> SystemParam for Client<'a, P> {
    type Fetch = State<P>;
}
