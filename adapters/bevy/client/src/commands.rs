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

/// Extension methods on [`EntityCommands`] for client-side replication and
/// authority management.
///
/// Requires that the protocol was built with
/// `enable_client_authoritative_entities()`.
pub trait CommandsExt<'a> {
    /// Spawns a local-only duplicate of this entity without replication.
    ///
    /// Copies all components to a new entity. The new entity is not
    /// registered with the naia replication layer.
    fn local_duplicate(&'a mut self) -> Entity;

    /// Updates the [`Publicity`] for this client-owned entity.
    ///
    /// Queued as a Bevy command — takes effect at the next
    /// `apply_deferred` boundary.
    fn configure_replication<T: Send + Sync + 'static>(
        &'a mut self,
        config: Publicity,
    ) -> &'a mut EntityCommands<'a>;

    /// Registers this entity with the naia replication layer.
    ///
    /// After this call, inserting `#[derive(Replicate)]` components will
    /// begin tracking and transmission to the server.
    fn enable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;

    /// Registers this entity as static with the naia replication layer.
    ///
    /// A full component snapshot is sent once when the entity enters the
    /// server's scope. No diff-tracking occurs afterward. Use for
    /// client-authoritative entities that are write-once after spawn
    /// (e.g. tiles or level geometry sent from client to server).
    ///
    /// Does not require a `&mut Client` — queued as a Bevy command.
    fn as_static<T: Send + Sync + 'static>(&'a mut self) -> &'a mut EntityCommands<'a>;

    /// Removes this entity from the naia replication layer.
    fn disable_replication<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;

    /// Returns the current [`Publicity`] for this entity, or `None` if
    /// the entity is not registered.
    fn replication_config<T: Send + Sync + 'static>(
        &'a self,
        client: &Client<T>,
    ) -> Option<Publicity>;

    /// Sends an authority request to the server for this delegated entity.
    ///
    /// The server responds with an `EntityAuthGrantedEvent` or
    /// `EntityAuthDeniedEvent`.
    fn request_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;

    /// Releases the client's authority over this entity back to the server.
    fn release_authority<T: Send + Sync + 'static>(
        &'a mut self,
        client: &mut Client<T>,
    ) -> &'a mut EntityCommands<'a>;

    /// Returns the current authority status for this entity, or `None`
    /// if the entity is not delegable.
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

    fn as_static<T: Send + Sync + 'static>(&'a mut self) -> &'a mut EntityCommands<'a> {
        let entity = self.id();
        self.insert(HostOwned::new::<T>());
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|_world, mut client: Mut<ClientWrapper<T>>| {
                client.client.enable_static_entity_replication(&entity);
            });
        }));
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

/// Extension methods on [`Commands`] for client-side replicated resource
/// authority management.
///
/// All methods queue Bevy commands that run at the next `apply_deferred`
/// boundary.
pub trait ClientCommandsExt {
    /// Requests authority on a delegable server-replicated resource.
    ///
    /// The server must have configured the resource with
    /// `ReplicationConfig::delegated()` via
    /// [`ServerCommandsExt::configure_replicated_resource`]. The server's
    /// response (Granted or Denied) arrives asynchronously as part of the
    /// normal authority-channel flow. Once `Granted`, mutations via
    /// `ResMut<R>` propagate back to the server.
    ///
    /// [`ServerCommandsExt::configure_replicated_resource`]: naia_bevy_server::ServerCommandsExt::configure_replicated_resource
    fn request_resource_authority<T, R>(&mut self)
    where
        T: Send + Sync + 'static,
        R: ReplicatedResource;

    /// Releases the client's authority over a previously granted resource.
    ///
    /// The server resumes exclusive ownership after confirming the release.
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
