use bevy_ecs::{
    entity::Entity,
    system::{Commands, EntityCommands},
    world::{Mut, World},
};
use naia_bevy_shared::{
    AuthorityError, EntityAuthStatus, HostOwned, ReplicatedResource, WorldMutType, WorldOpCommand,
    WorldProxyMut,
};
use naia_client::Publicity;

use crate::{client::ClientWrapper, Client};

// =====================================================================
// EntityCommands extension
// =====================================================================

pub trait CommandsExt<'a> {
    fn local_duplicate(&'a mut self) -> Entity;
    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: Publicity,
    ) -> &'a mut EntityCommands<'a>;
    fn enable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;
    fn disable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;
    fn replication_config<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<Publicity>;
    fn request_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;
    fn release_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;
    fn authority<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<EntityAuthStatus>;
}

impl<'a> CommandsExt<'a> for EntityCommands<'a> {
    fn local_duplicate(&'a mut self) -> Entity {
        let old_entity = self.id();
        let mut commands = self.commands();
        let new_entity = commands.spawn_empty().id();
        commands.queue(WorldOpCommand::new(move |world| {
            WorldMutType::<Entity>::local_duplicate_components(
                &mut world.proxy_mut(),
                &new_entity,
                &old_entity,
            );
        }));
        new_entity
    }

    fn enable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a> {
        client.enable_replication(&self.id());
        self.insert(HostOwned::new::<T>());
        self
    }

    fn disable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a> {
        client.disable_replication(&self.id());
        self.remove::<HostOwned>();
        self
    }

    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: Publicity,
    ) -> &'a mut EntityCommands<'a> {
        let entity = self.id();
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
                client.client.configure_entity_replication(
                    &mut world.proxy_mut(),
                    &entity,
                    config,
                );
            });
        }));
        self
    }

    fn replication_config<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<Publicity> {
        client.replication_config(&self.id())
    }

    fn request_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a> {
        client.entity_request_authority(&self.id());
        self
    }

    fn release_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a> {
        client.entity_release_authority(&self.id());
        self
    }

    fn authority<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<EntityAuthStatus> {
        client.entity_authority_status(&self.id())
    }
}

// =====================================================================
// Replicated Resources — Commands extension (client side)
// =====================================================================
//
// Mirror of the server's ServerCommandsExt. The user-visible client
// API for delegated resources is:
//
//   commands.request_resource_authority::<MyClient, MyResource>();
//   commands.release_resource_authority::<MyClient, MyResource>();
//   client.resource_authority_status::<MyResource>() -> Option<EntityAuthStatus>
//
// Each method queues a Bevy `Command` (via the shared
// `WorldOpCommand` helper) that runs with `&mut World` and dispatches
// into `ClientWrapper<T>` via `world.resource_scope`.

pub trait ClientCommandsExt {
    /// Request authority on a delegable resource of type `R`. The
    /// request is sent to the server; the response (Granted/Denied)
    /// arrives later as part of the normal authority-channel flow.
    /// Once Granted, mutations via `ResMut<R>` propagate to the server.
    fn request_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource;

    /// Release authority on a previously-granted resource.
    fn release_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource;
}

impl<'w, 's> ClientCommandsExt for Commands<'w, 's> {
    fn request_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource,
    {
        self.queue(WorldOpCommand::new(move |world| {
            let Some(entity) = find_resource_entity::<T, R>(world) else {
                log::warn!(
                    "naia request_resource_authority: resource not present in client world; skipping"
                );
                return;
            };
            world.resource_scope(|_world, mut client: Mut<ClientWrapper<T>>| {
                match client.client.entity_request_authority(&entity) {
                    Ok(()) => {}
                    Err(AuthorityError::NotDelegated) => {
                        log::warn!(
                            "naia request_resource_authority: resource not configured for delegation"
                        );
                    }
                    Err(e) => {
                        log::warn!("naia request_resource_authority failed: {:?}", e);
                    }
                }
            });
        }));
    }

    fn release_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource,
    {
        self.queue(WorldOpCommand::new(move |world| {
            let Some(entity) = find_resource_entity::<T, R>(world) else {
                return;
            };
            world.resource_scope(|_world, mut client: Mut<ClientWrapper<T>>| {
                let _ = client.client.entity_release_authority(&entity);
            });
        }));
    }
}

/// O(1) lookup of the resource entity for `R` via the client's
/// `ResourceRegistry`. Returns `None` if the resource isn't currently
/// in scope.
fn find_resource_entity<T, R>(world: &World) -> Option<Entity>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    world
        .get_resource::<ClientWrapper<T>>()
        .and_then(|cw| cw.client.resource_entity::<R>())
}
