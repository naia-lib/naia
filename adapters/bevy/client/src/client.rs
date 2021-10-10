use std::{collections::VecDeque, marker::PhantomData};

use bevy::ecs::{
    system::SystemParam,
    world::{Mut, World},
};

use naia_client::{Client as NaiaClient, NaiaClientError, ProtocolType, Event, ImplRef, EntityRef};

use naia_bevy_shared::{Entity, WorldProxy, WorldRef};

use super::{state::State, ticker::Ticker};

// Client

pub struct Client<'a, P: ProtocolType> {
    state: &'a mut State<P>,
    world: &'a World,
    client: Mut<'a, NaiaClient<P, Entity>>,
    ticker: Mut<'a, Ticker>,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Client<'a, P> {
    // Public Methods //

    pub fn new(state: &'a mut State<P>, world: &'a World) -> Self {
        unsafe {
            let client = world
                .get_resource_unchecked_mut::<NaiaClient<P, Entity>>()
                .expect("Naia Client has not been correctly initialized!");

            let ticker = world
                .get_resource_unchecked_mut::<Ticker>()
                .expect("Naia Client has not been correctly initialized!");

            Self {
                state,
                world,
                client,
                ticker,
                phantom_p: PhantomData,
            }
        }
    }

    pub fn receive(&mut self) -> VecDeque<Result<Event<P, Entity>, NaiaClientError>> {
        return self.client.receive(self.world.proxy());
    }

    //// Messages ////
    pub fn queue_message<R: ImplRef<P>>(
        &mut self,
        message_ref: &R,
        guaranteed_delivery: bool,
    ) {
        return self
            .client
            .queue_message(message_ref, guaranteed_delivery);
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
        self.ticker.ticked = true;
    }

    pub fn tick_finish(&mut self) {
        self.ticker.ticked = false;
    }

    pub fn has_ticked(&self) -> bool {
        return self.ticker.ticked;
    }
}

impl<'a, P: ProtocolType> SystemParam for Client<'a, P> {
    type Fetch = State<P>;
}
