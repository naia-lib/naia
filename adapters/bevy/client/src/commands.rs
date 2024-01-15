use std::{any::Any, marker::PhantomData};

use bevy_ecs::{
    entity::Entity,
    system::{Command as BevyCommand, EntityCommands},
    world::{Mut, World},
};

use naia_bevy_shared::{EntityAuthStatus, HostOwned, WorldMutType, WorldProxyMut};
use naia_client::ReplicationConfig;

use crate::{Client, client::ClientWrapper};

// Bevy Commands Extension
pub trait CommandsExt<'w, 's, 'a> {
    fn local_duplicate(&'a mut self) -> EntityCommands<'w, 's, 'a>;
    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn enable_replication<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn disable_replication<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>)
        -> &'a mut EntityCommands<'w, 's, 'a>;
    fn replication_config<T: Send + Sync + 'static>(&'a self, client: &Client<T>) -> Option<ReplicationConfig>;
    fn request_authority<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn release_authority<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn authority<T: Send + Sync + 'static>(&'a self, client: &Client<T>) -> Option<EntityAuthStatus>;
}

impl<'w, 's, 'a> CommandsExt<'w, 's, 'a> for EntityCommands<'w, 's, 'a> {
    fn local_duplicate(&'a mut self) -> EntityCommands<'w, 's, 'a> {
        let old_entity = self.id();
        let commands = self.commands();
        let new_entity = commands.spawn_empty().id();
        let command = LocalDuplicateComponents::new(new_entity, old_entity);
        commands.add(command);
        commands.entity(new_entity)
    }

    fn enable_replication<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.enable_replication(&self.id());
        self.insert(HostOwned::new::<T>());
        return self;
    }

    fn disable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.disable_replication(&self.id());
        self.remove::<HostOwned>();
        return self;
    }

    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        let entity = self.id();
        let commands = self.commands();
        let command = ConfigureReplicationCommand::<T>::new(entity, config);
        commands.add(command);
        return self;
    }

    fn replication_config<T: Send + Sync + 'static>(&'a self, client: &Client<T>) -> Option<ReplicationConfig> {
        client.replication_config(&self.id())
    }

    fn request_authority<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.entity_request_authority(&self.id());
        return self;
    }

    fn release_authority<T: Send + Sync + 'static>(&'a mut self, client: &mut Client<T>) -> &'a mut EntityCommands<'w, 's, 'a> {
        client.entity_release_authority(&self.id());
        return self;
    }

    fn authority<T: Send + Sync + 'static>(&'a self, client: &Client<T>) -> Option<EntityAuthStatus> {
        client.entity_authority_status(&self.id())
    }
}

//// LocalDuplicateComponents Command ////
pub(crate) struct LocalDuplicateComponents {
    mutable_entity: Entity,
    immutable_entity: Entity,
}

impl LocalDuplicateComponents {
    pub fn new(new_entity: Entity, old_entity: Entity) -> Self {
        Self {
            mutable_entity: new_entity,
            immutable_entity: old_entity,
        }
    }
}

impl BevyCommand for LocalDuplicateComponents {
    fn apply(self, world: &mut World) {
        WorldMutType::<Entity>::local_duplicate_components(
            &mut world.proxy_mut(),
            &self.mutable_entity,
            &self.immutable_entity,
        );
    }
}

//// ConfigureReplicationCommand Command ////
pub(crate) struct ConfigureReplicationCommand<T: Send + Sync + 'static> {
    entity: Entity,
    config: ReplicationConfig,
    phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> ConfigureReplicationCommand<T> {
    pub fn new(entity: Entity, config: ReplicationConfig) -> Self {
        Self { entity, config, phantom_t: PhantomData }
    }
}

impl<T: Send + Sync + 'static> BevyCommand for ConfigureReplicationCommand<T> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
            client.client.configure_entity_replication(&mut world.proxy_mut(), &self.entity, self.config);
        });
    }
}
