use std::marker::PhantomData;

use bevy::{ecs::{world::{World, Mut}, system::{SystemParamState, SystemParamFetch, SystemState}}};

use naia_server::{ProtocolType, Server as NaiaServer};

use crate::world::{entity::Entity, world_proxy::WorldProxyMut};

use super::{server::Server, commands::Command};

// State

pub struct State<P: ProtocolType> {
    commands: Vec<Box<dyn Command<P>>>,
    phantom_p: PhantomData<P>,
}

impl<P: ProtocolType> State<P> {
    pub fn apply(&mut self, world: &mut World) {
        // Have to do this to get around 'world.flush()' only being crate-public
        world.spawn().despawn();

        // resource scope
        world.resource_scope(|world: &mut World, mut server: Mut<NaiaServer<P, Entity>>| {
            let world_proxy = &mut world.proxy_mut();

            // Process queued commands
            for command in self.commands.drain(..) {
                command.write(&mut server, world_proxy);
            }
        });
    }

    #[inline]
    pub fn push_boxed(&mut self, command: Box<dyn Command<P>>) {
        self.commands.push(command);
    }

    #[inline]
    pub fn push<T: Command<P>>(&mut self, command: T) {
        self.push_boxed(Box::new(command));
    }
}

// SAFE: only local state is accessed
unsafe impl<P: ProtocolType> SystemParamState for State<P> {
    type Config = ();

    fn init(_world: &mut World, _system_state: &mut SystemState, _config: Self::Config) -> Self {
        State {
            commands: Vec::new(),
            phantom_p: PhantomData,
        }
    }

    fn apply(&mut self, world: &mut World) {
        self.apply(world);
    }

    fn default_config() {}
}

impl<'a, P: ProtocolType> SystemParamFetch<'a> for State<P> {
    type Item = Server<'a, P>;

    #[inline]
    unsafe fn get_param(
        state: &'a mut Self,
        _system_state: &'a SystemState,
        world: &'a World,
        _change_tick: u32,
    ) -> Self::Item {
        Server::new(state, world)
    }
}