use std::marker::PhantomData;

use bevy_ecs::system::Command;
use bevy_ecs::{
    entity::Entity,
    system::{Commands, EntityCommands},
    world::{Mut, World},
};
use naia_bevy_shared::{
    AuthorityError, EntityAuthStatus, HostOwned, ReplicatedResource, WorldMutType, WorldProxyMut,
};
use naia_client::ReplicationConfig;

use crate::{client::ClientWrapper, Client};

// Bevy Commands Extension
pub trait CommandsExt<'a> {
    fn local_duplicate(&'a mut self) -> Entity;
    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: ReplicationConfig,
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
    ) -> Option<ReplicationConfig>;
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
        let command = LocalDuplicateComponents::new(new_entity, old_entity);
        commands.queue(command);
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
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'a> {
        let entity = self.id();
        let mut commands = self.commands();
        let command = ConfigureReplicationCommand::<T>::new(entity, config);
        commands.queue(command);
        self
    }

    fn replication_config<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<ReplicationConfig> {
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

impl Command for LocalDuplicateComponents {
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
        Self {
            entity,
            config,
            phantom_t: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static> Command for ConfigureReplicationCommand<T> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut client: Mut<ClientWrapper<T>>| {
            client.client.configure_entity_replication(
                &mut world.proxy_mut(),
                &self.entity,
                self.config,
            );
        });
    }
}

// =====================================================================
// Replicated Resources — Commands extension (client side, R8)
// =====================================================================
//
// Mirror of the server's ServerCommandsExt (R7). The user-visible client
// API for delegated resources is:
//
//   commands.request_resource_authority::<MyClient, MyResource>();
//   commands.release_resource_authority::<MyClient, MyResource>();
//   client.resource_authority_status::<MyResource>() -> Option<EntityAuthStatus>
//
// Implementation note: until the proper client-side ResourceRegistry
// lands (paired with the Mode B mirror system), the client locates the
// resource entity by scanning the client's world for an entity carrying
// `R` as a component. Operationally identical to the eventual registry
// lookup; only the asymptotic cost differs (O(n) entities vs O(1) for
// V1, then O(1) post-Mode-B). For typical resource counts (< 100) the
// scan is sub-microsecond.
//
// The Commands queue dispatch pattern matches `ConfigureReplicationCommand`
// above — runs with `&mut World`, dispatches via `world.resource_scope`.

pub trait ClientCommandsExt {
    /// Request authority on a delegable resource of type `R`. The
    /// request is sent to the server; the response (Granted/Denied)
    /// arrives later as part of the normal authority-channel flow.
    /// Once Granted, mutations via `Query<&mut R>` over the resource
    /// entity propagate back to the server.
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
        self.queue(RequestResourceAuthorityCommand::<T, R>::new());
    }

    fn release_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource,
    {
        self.queue(ReleaseResourceAuthorityCommand::<T, R>::new());
    }
}

/// O(1) lookup of the resource entity for `R` via the client's
/// `ResourceRegistry`. Returns `None` if the resource isn't currently
/// in scope. Replaces the V1 world-scan (A1 of RESOURCES_AUDIT.md).
fn find_resource_entity<T, R>(world: &World) -> Option<Entity>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    world
        .get_resource::<ClientWrapper<T>>()
        .and_then(|cw| cw.client.resource_entity::<R>())
}

//// RequestResourceAuthorityCommand ////
pub(crate) struct RequestResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

impl<T, R> RequestResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    pub fn new() -> Self {
        Self {
            _phantom_t: PhantomData,
            _phantom_r: PhantomData,
        }
    }
}

impl<T, R> Command for RequestResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    fn apply(self, world: &mut World) {
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
    }
}

//// ReleaseResourceAuthorityCommand ////
pub(crate) struct ReleaseResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    _phantom_t: PhantomData<T>,
    _phantom_r: PhantomData<R>,
}

impl<T, R> ReleaseResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    pub fn new() -> Self {
        Self {
            _phantom_t: PhantomData,
            _phantom_r: PhantomData,
        }
    }
}

impl<T, R> Command for ReleaseResourceAuthorityCommand<T, R>
where
    T: Send + Sync + 'static,
    R: ReplicatedResource,
{
    fn apply(self, world: &mut World) {
        let Some(entity) = find_resource_entity::<T, R>(world) else {
            return;
        };
        world.resource_scope(|_world, mut client: Mut<ClientWrapper<T>>| {
            let _ = client.client.entity_release_authority(&entity);
        });
    }
}
