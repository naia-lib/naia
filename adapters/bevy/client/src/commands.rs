use bevy_ecs::{
    entity::Entity,
    system::{Command as BevyCommand, EntityCommands},
    world::World,
};

use naia_bevy_shared::{HostOwned, WorldMutType, WorldProxyMut};

use crate::{Client, ReplicationConfig};

// Bevy Commands Extension
pub trait CommandsExt<'w, 's, 'a> {
    fn enable_replication(&'a mut self, client: &mut Client) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn disable_replication(&'a mut self, client: &mut Client)
        -> &'a mut EntityCommands<'w, 's, 'a>;
    fn configure_replication(
        &'a mut self,
        client: &mut Client,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn replication_config(&'a self, client: &Client) -> ReplicationConfig;
    fn duplicate(&'a mut self) -> EntityCommands<'w, 's, 'a>;
}

impl<'w, 's, 'a> CommandsExt<'w, 's, 'a> for EntityCommands<'w, 's, 'a> {
    fn enable_replication(&'a mut self, client: &mut Client) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.enable_replication(&self.id());
        self.insert(HostOwned);
        return self;
    }

    fn disable_replication(
        &'a mut self,
        client: &mut Client,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.disable_replication(&self.id());
        self.remove::<HostOwned>();
        return self;
    }

    fn configure_replication(
        &'a mut self,
        client: &mut Client,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        match &config {
            ReplicationConfig::Disabled => {
                self.remove::<HostOwned>();
            }
            _ => {
                self.insert(HostOwned);
            }
        }
        client.configure_replication(&self.id(), config);
        return self;
    }

    fn replication_config(&'a self, client: &Client) -> ReplicationConfig {
        client.replication_config(&self.id())
    }

    fn duplicate(&'a mut self) -> EntityCommands<'w, 's, 'a> {
        let old_entity = self.id();
        let commands = self.commands();
        let new_entity = commands.spawn_empty().id();
        let command = DuplicateComponents::new(new_entity, old_entity);
        commands.add(command);
        commands.entity(new_entity)
    }
}

//// DuplicateComponents Command ////

pub(crate) struct DuplicateComponents {
    mutable_entity: Entity,
    immutable_entity: Entity,
}

impl DuplicateComponents {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
        }
    }
}

impl BevyCommand for DuplicateComponents {
    fn write(self, world: &mut World) {
        WorldMutType::<Entity>::duplicate_components(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}
