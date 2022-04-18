use bevy_ecs::{
    entity::Entity,
    system::{SystemMeta, SystemParamFetch, SystemParamState},
    world::{Mut, World},
};

use naia_server::{
    shared::{ChannelIndex, Protocolize},
    Server as NaiaServer,
};

use naia_bevy_shared::WorldProxyMut;

use super::{commands::Command, server::Server};

// State

pub struct State<P: Protocolize, C: ChannelIndex> {
    commands: Vec<Box<dyn Command<P, C>>>,
}

impl<P: Protocolize, C: ChannelIndex> State<P, C> {
    pub fn apply(&mut self, world: &mut World) {
        // Have to do this to get around 'world.flush()' only being crate-public
        world.spawn().despawn();

        // resource scope
        world.resource_scope(
            |world: &mut World, mut server: Mut<NaiaServer<P, Entity, C>>| {
                // Process queued commands
                for command in self.commands.drain(..) {
                    command.write(&mut server, world.proxy_mut());
                }
            },
        );
    }

    #[inline]
    pub fn push_boxed(&mut self, command: Box<dyn Command<P, C>>) {
        self.commands.push(command);
    }

    #[inline]
    pub fn push<T: Command<P, C>>(&mut self, command: T) {
        self.push_boxed(Box::new(command));
    }
}

// SAFE: only local state is accessed
unsafe impl<P: Protocolize, C: ChannelIndex> SystemParamState for State<P, C> {
    fn init(_world: &mut World, _system_meta: &mut SystemMeta) -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    fn apply(&mut self, world: &mut World) {
        self.apply(world);
    }
}

impl<'world, 'state, P: Protocolize, C: ChannelIndex> SystemParamFetch<'world, 'state>
    for State<P, C>
{
    type Item = Server<'world, 'state, P, C>;

    #[inline]
    unsafe fn get_param(
        state: &'state mut Self,
        _system_state: &SystemMeta,
        world: &'world World,
        _change_tick: u32,
    ) -> Self::Item {
        Server::new(state, world)
    }
}
