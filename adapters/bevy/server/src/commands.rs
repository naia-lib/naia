use bevy_ecs::{
    entity::Entity,
    system::EntityCommands,
    world::{Command as BevyCommand, Mut, World},
};

use naia_bevy_shared::{HostOwned};
use naia_server::{ReplicationConfig, UserKey};

use crate::{world_entity::WorldId, world_proxy::WorldProxyMut, world_entity::WorldEntity, plugin::Singleton, server::ServerWrapper};

// Bevy Commands Extension
pub trait CommandsExt<'a> {
    // basic
    fn enable_replication(&'a mut self) -> &'a mut EntityCommands<'a>;
    fn disable_replication(&'a mut self) -> &'a mut EntityCommands<'a>;
    fn pause_replication(&'a mut self) -> &'a mut EntityCommands<'a>;
    fn resume_replication(&'a mut self) -> &'a mut EntityCommands<'a>;

    // authority related
    fn configure_replication(&'a mut self, config: ReplicationConfig) -> &'a mut EntityCommands<'a>;
    fn give_authority(
        &'a mut self,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a>;
    fn take_authority(&'a mut self) -> &'a mut EntityCommands<'a>;
}

impl<'a> CommandsExt<'a> for EntityCommands<'a> {
    fn enable_replication(&'a mut self) -> &'a mut EntityCommands<'a> {

        let entity = self.id();

        let mut commands = self.commands();
        let command = EnableReplicationCommand::new(entity);
        commands.queue(command);

        self.insert(HostOwned::new::<Singleton>());

        self
    }

    fn disable_replication(&'a mut self) -> &'a mut EntityCommands<'a> {

        let entity = self.id();

        let mut commands = self.commands();
        let command = DisableReplicationCommand::new(entity);
        commands.queue(command);

        self.remove::<HostOwned>();

        self
    }

    fn pause_replication(&'a mut self) -> &'a mut EntityCommands<'a> {

        let entity = self.id();

        let mut commands = self.commands();
        let command = PauseReplicationCommand::new(entity);
        commands.queue(command);

        self
    }

    fn resume_replication(&'a mut self) -> &'a mut EntityCommands<'a> {

        let entity = self.id();

        let mut commands = self.commands();
        let command = ResumeReplicationCommand::new(entity);
        commands.queue(command);

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

    fn give_authority(
        &'a mut self,
        _user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a> {
        todo!()
    }

    fn take_authority(&'a mut self) -> &'a mut EntityCommands<'a> {

        let entity = self.id();

        let mut commands = self.commands();
        let command = TakeAuthorityCommand::new(entity);
        commands.queue(command);

        self
    }
}

fn get_world_id(world: &World) -> WorldId {
    let world_id = world.get_resource::<WorldId>().unwrap();
    *world_id
}

fn get_world_entity(world: &World, entity: &Entity) -> WorldEntity {
    let world_id = get_world_id(world);
    WorldEntity::new(world_id, *entity)
}

//// EnableReplicationCommand Command ////

pub(crate) struct EnableReplicationCommand {
    entity: Entity,
}

impl EnableReplicationCommand {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl BevyCommand for EnableReplicationCommand {
    fn apply(self, world: &mut World) {
        let world_entity = get_world_entity(world, &self.entity);
        let mut server_wrapper = world.get_resource_mut::<ServerWrapper>().unwrap();
        server_wrapper.enable_replication(&world_entity);
    }
}

//// DisableReplicationCommand Command ////

pub(crate) struct DisableReplicationCommand {
    entity: Entity,
}

impl DisableReplicationCommand {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl BevyCommand for DisableReplicationCommand {
    fn apply(self, world: &mut World) {
        let world_entity = get_world_entity(world, &self.entity);
        let mut server_wrapper = world.get_resource_mut::<ServerWrapper>().unwrap();
        server_wrapper.disable_replication(&world_entity);
    }
}

//// PauseReplicationCommand Command ////

pub(crate) struct PauseReplicationCommand {
    entity: Entity,
}

impl PauseReplicationCommand {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl BevyCommand for PauseReplicationCommand {
    fn apply(self, world: &mut World) {
        let world_entity = get_world_entity(world, &self.entity);
        let mut server_wrapper = world.get_resource_mut::<ServerWrapper>().unwrap();
        server_wrapper.pause_replication(&world_entity);
    }
}

//// ResumeReplicationCommand Command ////

pub(crate) struct ResumeReplicationCommand {
    entity: Entity,
}

impl ResumeReplicationCommand {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl BevyCommand for ResumeReplicationCommand {
    fn apply(self, world: &mut World) {
        let world_entity = get_world_entity(world, &self.entity);
        let mut server_wrapper = world.get_resource_mut::<ServerWrapper>().unwrap();
        server_wrapper.resume_replication(&world_entity);
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
        let world_entity = get_world_entity(world, &self.entity);

        world.resource_scope(|world, mut server: Mut<ServerWrapper>| {
            server.set_replication_config(
                &mut world.proxy_mut(),
                &world_entity,
                self.config,
            );
        });
    }
}

//// TakeAuthorityCommand Command ////

pub(crate) struct TakeAuthorityCommand {
    entity: Entity,
}

impl TakeAuthorityCommand {
    pub fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

impl BevyCommand for TakeAuthorityCommand {
    fn apply(self, world: &mut World) {
        let world_entity = get_world_entity(world, &self.entity);
        let mut server_wrapper = world.get_resource_mut::<ServerWrapper>().unwrap();
        server_wrapper.entity_take_authority(&world_entity);
    }
}