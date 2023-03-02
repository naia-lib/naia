use bevy_ecs::{
    entity::Entity,
    system::{SystemMeta, SystemParamFetch, SystemParamState},
    world::{Mut, World},
};

use naia_client::Client as NaiaClient;

use naia_bevy_shared::WorldProxyMut;

use super::{client::Client, commands::Command};

// State

pub struct State {
    commands: Vec<Box<dyn Command>>,
}

impl State {
    pub fn apply(&mut self, world: &mut World) {
        // Have to do this to get around 'world.flush()' only being crate-public
        world.spawn_empty().despawn();

        // resource scope
        world.resource_scope(|world: &mut World, mut client: Mut<NaiaClient<Entity>>| {
            // Process queued commands
            for command in self.commands.drain(..) {
                command.write(&mut client, world.proxy_mut());
            }
        });
    }

    #[inline]
    pub fn push_boxed(&mut self, command: Box<dyn Command>) {
        self.commands.push(command);
    }

    #[inline]
    pub fn push<T: Command>(&mut self, command: T) {
        self.push_boxed(Box::new(command));
    }
}

// SAFE: only local state is accessed
unsafe impl SystemParamState for State {
    fn init(_world: &mut World, _system_meta: &mut SystemMeta) -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    fn apply(&mut self, world: &mut World) {
        self.apply(world);
    }
}

impl<'world, 'state> SystemParamFetch<'world, 'state> for State {
    type Item = Client<'world, 'state>;

    #[inline]
    unsafe fn get_param(
        state: &'state mut Self,
        _system_state: &SystemMeta,
        world: &'world World,
        _change_tick: u32,
    ) -> Self::Item {
        Client::new(state, world)
    }
}
