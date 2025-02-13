use bevy_ecs::{
    entity::Entity,
    system::EntityCommands,
    world::{Command as BevyCommand, Mut, World},
};

use naia_bevy_shared::{EntityAuthStatus, HostOwned};
use naia_server::{ReplicationConfig, UserKey};

use crate::{world_entity::WorldId, world_proxy::WorldProxyMut, world_entity::WorldEntity, plugin::Singleton, server::ServerWrapper, Server};

// Bevy Commands Extension
pub trait CommandsExt<'a> {

    // Replication

    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    // Authority
    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig>;
    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus>;

    fn configure_replication(&'a mut self, server: &Server, config: ReplicationConfig) -> &'a mut EntityCommands<'a>;
    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a>;
    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;
}

impl<'a> CommandsExt<'a> for EntityCommands<'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.enable_replication(&self.id());
        self.insert(HostOwned::new::<Singleton>());
        self
    }

    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.disable_replication(&self.id());
        self.remove::<HostOwned>();
        self
    }

    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.pause_replication(&self.id());
        self
    }

    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.resume_replication(&self.id());
        self
    }

    // Authority

    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig> {
        server.replication_config(&self.id())
    }

    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus> {
        server.entity_authority_status(&self.id())
    }

    fn configure_replication(
        &'a mut self,
        server: &Server,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'a> {
        let world_id = server.world_id();
        let entity = self.id();
        let mut commands = self.commands();
        let command = ConfigureReplicationCommand::new(world_id, entity, config);
        commands.queue(command);
        self
    }

    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a> {
        server.entity_give_authority(user_key, &self.id());
        self
    }

    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.entity_take_authority(&self.id());
        self
    }
}

//// ConfigureReplicationCommand Command ////

pub(crate) struct ConfigureReplicationCommand {
    world_id: WorldId,
    entity: Entity,
    config: ReplicationConfig,
}

impl ConfigureReplicationCommand {
    pub fn new(world_id: WorldId, entity: Entity, config: ReplicationConfig) -> Self {
        Self { world_id, entity, config }
    }
}

impl BevyCommand for ConfigureReplicationCommand {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerWrapper>| {
            let world_entity = WorldEntity::new(self.world_id, self.entity);
            server.configure_entity_replication(
                &mut world.proxy_mut(),
                &world_entity,
                self.config,
            );
        });
    }
}