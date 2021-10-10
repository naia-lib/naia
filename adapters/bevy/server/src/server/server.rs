use std::marker::PhantomData;

use bevy::{ecs::{world::World, system::SystemParam}};

use naia_server::ProtocolType;

use crate::world::entity::Entity;

use super::{entity_mut::EntityMut, state::State, commands::Command};

// Server

pub struct Server<'a, P: ProtocolType> {
    state: &'a mut State<P>,
    world: &'a World,
    phantom_p: PhantomData<P>,
}

impl<'a, P: ProtocolType> Server<'a, P> {
    pub fn new(state: &'a mut State<P>, world: &'a World) -> Self {
        Self {
            state,
            world,
            phantom_p: PhantomData,
        }
    }

    pub fn spawn(&mut self) -> EntityMut<'a, '_, P> {
        let entity = self.world.entities().reserve_entity();
        EntityMut::new(
            Entity::new(entity),
            self,
        )
    }

    pub fn entity(&mut self, entity: &Entity) -> EntityMut<'a, '_, P> {
        EntityMut::new(
            *entity,
            self,
        )
    }

    pub(crate) fn add<C: Command>(&mut self, command: C) {
        self.state.push(command);
    }
}

impl<'a, P: ProtocolType> SystemParam for Server<'a, P> {
    type Fetch = State<P>;
}