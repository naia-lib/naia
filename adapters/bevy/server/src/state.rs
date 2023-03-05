use bevy_ecs::{
    entity::Entity,
    system::{SystemMeta, SystemBuffer},
    world::{Mut, World},
};

use naia_server::Server as NaiaServer;

use naia_bevy_shared::WorldProxyMut;

use super::commands::Command;

// State
#[derive(Default)]
pub struct ServerCommandQueue {
    commands: Vec<Box<dyn Command>>
}

impl SystemBuffer for ServerCommandQueue {
    #[inline]
    fn apply(&mut self, _system_meta: &SystemMeta, world: &mut World) {
        #[cfg(feature = "trace")]
            let _system_span =
            bevy_utils::tracing::info_span!("system_commands", name = _system_meta.name())
                .entered();
        self.apply(world);
    }
}

impl ServerCommandQueue {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn apply(&mut self, world: &mut World) {
        // Have to do this to get around 'world.flush()' only being crate-public
        world.spawn_empty().despawn();

        // resource scope
        world.resource_scope(|world: &mut World, mut server: Mut<NaiaServer<Entity>>| {
            // Process queued commands
            for command in self.commands.drain(..) {
                command.write(&mut server, world.proxy_mut());
            }
        });
    }

    fn push_boxed(&mut self, command: Box<dyn Command>) {
        self.commands.push(command);
    }

    pub fn push<T: Command>(&mut self, command: T) {
        self.push_boxed(Box::new(command));
    }
}