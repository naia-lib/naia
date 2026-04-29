use bevy_ecs::system::Command;
use bevy_ecs::{
    entity::Entity,
    system::EntityCommands,
    world::{Mut, World},
};
use naia_bevy_shared::{EntityAuthStatus, HostOwned, WorldProxyMut};
use naia_server::{ReplicationConfig, UserKey};

use crate::{plugin::Singleton, server::ServerImpl, Server};

// Bevy Commands Extension
pub trait CommandsExt<'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    /// Like `enable_replication` but marks the entity as static — IDs from the
    /// static pool, no diff-tracking after initial replication, post-spawn
    /// mutation panics. Use for tile entities and other frozen scenery.
    fn enable_static_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn configure_replication(&'a mut self, config: ReplicationConfig)
        -> &'a mut EntityCommands<'a>;
    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig>;
    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a>;
    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus>;
    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
}

impl<'a> CommandsExt<'a> for EntityCommands<'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.enable_replication(&self.id());
        self.insert(HostOwned::new::<Singleton>());
        self
    }

    fn enable_static_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.enable_static_replication(&self.id());
        self.insert(HostOwned::new::<Singleton>());
        self
    }

    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.disable_replication(&self.id());
        self.remove::<HostOwned>();
        self
    }

    fn configure_replication(
        &'a mut self,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'a> {
        let entity = self.id();
        let mut commands = self.commands();
        let command = ConfigureReplicationCommand::new(entity, config);
        commands.queue(command);
        self
    }

    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig> {
        server.replication_config(&self.id())
    }

    fn give_authority(
        &'a mut self,
        _server: &mut Server,
        _user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a> {
        todo!()
    }

    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.entity_take_authority(&self.id());
        return self;
    }

    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus> {
        server.entity_authority_status(&self.id())
    }

    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.pause_replication(&self.id());
        return self;
    }

    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
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

impl Command for ConfigureReplicationCommand {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            server.configure_entity_replication(&mut world.proxy_mut(), &self.entity, self.config);
        });
    }
}
