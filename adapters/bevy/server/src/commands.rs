use bevy_ecs::{
    entity::Entity,
    system::{Command as BevyCommand, EntityCommands},
    world::{Mut, World},
};

use naia_bevy_shared::{EntityAuthStatus, HostOwned, WorldProxyMut};
use naia_server::{ReplicationConfig, Server as NaiaServer, UserKey};

use crate::Server;

// Bevy Commands Extension
pub trait CommandsExt<'w, 's, 'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn disable_replication(&'a mut self, server: &mut Server)
        -> &'a mut EntityCommands<'w, 's, 'a>;
    fn configure_replication(
        &'a mut self,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig>;
    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus>;
    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
}

impl<'w, 's, 'a> CommandsExt<'w, 's, 'a> for EntityCommands<'w, 's, 'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.enable_replication(&self.id());
        self.insert(HostOwned);
        return self;
    }

    fn disable_replication(
        &'a mut self,
        server: &mut Server,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.disable_replication(&self.id());
        self.remove::<HostOwned>();
        return self;
    }

    fn configure_replication(
        &'a mut self,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        let entity = self.id();
        let commands = self.commands();
        let command = ConfigureReplicationCommand::new(entity, config);
        commands.add(command);
        return self;
    }

    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig> {
        server.replication_config(&self.id())
    }

    fn give_authority(
        &'a mut self,
        _server: &mut Server,
        _user_key: &UserKey,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        todo!()
    }

    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.entity_take_authority(&self.id());
        return self;
    }

    fn authority(&'a self, _server: &Server) -> Option<EntityAuthStatus> {
        todo!()
    }

    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.pause_replication(&self.id());
        return self;
    }

    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.resume_replication(&self.id());
        return self;
    }
}

//// ConfigureReplicationCommand Command ////
pub(crate) struct ConfigureReplicationCommand {
    entity: Entity,
    config: ReplicationConfig,
}

impl ConfigureReplicationCommand {
    pub fn new(entity: Entity, config: ReplicationConfig) -> Self {
        Self { entity, config }
    }
}

impl BevyCommand for ConfigureReplicationCommand {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<NaiaServer<Entity>>| {
            server.configure_entity_replication(&mut world.proxy_mut(), &self.entity, self.config);
        });
    }
}
