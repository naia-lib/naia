use bevy_ecs::{
    system::{Commands, EntityCommands},
    world::Mut,
};
use naia_bevy_shared::{
    EntityAuthStatus, HostOwned, ReplicatedResource, WorldOpCommand, WorldProxyMut,
};
use naia_server::{ReplicationConfig, UserKey};

use crate::{plugin::Singleton, server::ServerImpl, Server};

// =====================================================================
// EntityCommands extension
// =====================================================================

/// Extension methods on [`EntityCommands`] for server-side replication and
/// authority management.
///
/// Import this trait and call its methods on `commands.entity(entity)`:
///
/// ```no_run
/// # use bevy_ecs::system::Commands;
/// # use naia_bevy_server::{CommandsExt, Server};
/// fn spawn_player(mut commands: Commands, mut server: bevy_ecs::system::ResMut<Server>) {
///     commands.spawn(/* … */)
///         .enable_replication(&mut server);
/// }
/// ```
pub trait CommandsExt<'a> {
    /// Registers the entity with the naia replication layer.
    ///
    /// After this call, inserting any `#[derive(Replicate)]` component
    /// on the entity will begin diff-tracking and replication to in-scope
    /// clients.
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Registers the entity as static — no diff-tracking after spawn.
    ///
    /// A full component snapshot is sent once when the entity enters a
    /// user's scope. Use for tile entities, level geometry, or any entity
    /// that never mutates after spawning.
    fn enable_static_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Removes the entity from the naia replication layer.
    ///
    /// Despawns the entity on all clients for whom it was in scope.
    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Updates the [`ReplicationConfig`] for this entity.
    ///
    /// Queued as a Bevy command — takes effect at the next
    /// `apply_deferred` boundary.
    fn configure_replication(&'a mut self, config: ReplicationConfig)
        -> &'a mut EntityCommands<'a>;

    /// Returns the current [`ReplicationConfig`] for this entity, or
    /// `None` if the entity is not registered.
    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig>;

    /// Grants authority over this entity to the given user.
    ///
    /// The entity must already have `Delegated` replication config.
    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a>;

    /// Reclaims server authority over this entity, revoking any client grant.
    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Returns the current authority status for this entity, or `None`
    /// if the entity is not delegable.
    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus>;

    /// Pauses replication for this entity without despawning it on clients.
    ///
    /// Component mutations are buffered but not transmitted until
    /// [`resume_replication`](CommandsExt::resume_replication) is called.
    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Resumes replication for an entity previously paused.
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
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut server: Mut<ServerImpl>| {
                server.configure_entity_replication(&mut world.proxy_mut(), &entity, config);
            });
        }));
        self
    }

    fn replication_config(&'a self, server: &Server) -> Option<ReplicationConfig> {
        server.replication_config(&self.id())
    }

    fn give_authority(
        &'a mut self,
        server: &mut Server,
        user_key: &UserKey,
    ) -> &'a mut EntityCommands<'a> {
        server.entity_give_authority(&self.id(), user_key);
        self
    }

    fn take_authority(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.entity_take_authority(&self.id());
        self
    }

    fn authority(&'a self, server: &Server) -> Option<EntityAuthStatus> {
        server.entity_authority_status(&self.id())
    }

    fn pause_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.pause_replication(&self.id());
        self
    }

    fn resume_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.resume_replication(&self.id());
        self
    }
}

// =====================================================================
// Replicated Resources — Commands extension
// =====================================================================
//
// User-facing API mirrors the entity-spawn split between dynamic and
// static ID pools. Each method queues a Bevy `Command` (via the shared
// `WorldOpCommand` helper) that runs with `&mut World` and dispatches
// into `ServerImpl` via `world.resource_scope`.
//
// Trait lives on `Commands<'_, '_>` (not `EntityCommands`) because
// resources have no user-visible entity identity.
//
// ## Deferral semantics
//
// Per Bevy's standard `Commands` queue: calls do NOT take effect
// immediately. They queue a `Command` that runs at the next
// `apply_deferred` boundary (typically end-of-stage). To observe the
// resource within the same system, schedule a follow-up system after
// an `apply_deferred` flush. This is standard Bevy behavior.

/// Extension methods on [`Commands`] for server-side replicated resource
/// management.
///
/// All methods queue Bevy commands that run at the next `apply_deferred`
/// boundary — changes do not take effect in the same system.
pub trait ServerCommandsExt {
    /// Inserts a dynamic (diff-tracked) replicated resource.
    ///
    /// The value is replicated to all connected clients. Subsequent
    /// mutations via `ResMut<R>` are diff-tracked and transmitted
    /// automatically.
    fn replicate_resource<R: ReplicatedResource>(&mut self, value: R);

    /// Inserts a static (immutable) replicated resource.
    ///
    /// A full snapshot is sent to each client once on connect. No
    /// diff-tracking occurs — the value must not change after insertion.
    fn replicate_resource_static<R: ReplicatedResource>(&mut self, value: R);

    /// Removes the replicated resource of type `R`.
    ///
    /// Despawns the hidden entity on all clients where it was in scope.
    fn remove_replicated_resource<R: ReplicatedResource>(&mut self);

    /// Updates the [`ReplicationConfig`] for the resource of type `R`.
    ///
    /// Use `ReplicationConfig::delegated()` to allow clients to request
    /// authority over the resource.
    fn configure_replicated_resource<R: ReplicatedResource>(&mut self, config: ReplicationConfig);
}

impl<'w, 's> ServerCommandsExt for Commands<'w, 's> {
    fn replicate_resource<R: ReplicatedResource>(&mut self, value: R) {
        let value_cell = parking_lot::Mutex::new(Some(value));
        self.queue(WorldOpCommand::new(move |world| {
            let value = value_cell
                .lock()
                .take()
                .expect("WorldOpCommand runs once");
            replicate_resource_inner::<R>(world, value, /* is_static */ false);
        }));
    }

    fn replicate_resource_static<R: ReplicatedResource>(&mut self, value: R) {
        let value_cell = parking_lot::Mutex::new(Some(value));
        self.queue(WorldOpCommand::new(move |world| {
            let value = value_cell
                .lock()
                .take()
                .expect("WorldOpCommand runs once");
            replicate_resource_inner::<R>(world, value, /* is_static */ true);
        }));
    }

    fn remove_replicated_resource<R: ReplicatedResource>(&mut self) {
        self.queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut server: Mut<ServerImpl>| {
                let _ = server.remove_resource::<_, R>(world.proxy_mut());
            });
        }));
    }

    fn configure_replicated_resource<R: ReplicatedResource>(&mut self, config: ReplicationConfig) {
        self.queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut server: Mut<ServerImpl>| {
                let _ = server.configure_resource::<_, R>(&mut world.proxy_mut(), config);
            });
        }));
    }
}

/// Shared body of `replicate_resource` and `replicate_resource_static`.
/// Inserts the resource on the entity-component side via ServerImpl,
/// then installs the Mode B bevy-Resource mirror.
fn replicate_resource_inner<R: ReplicatedResource>(
    world: &mut bevy_ecs::world::World,
    value: R,
    is_static: bool,
) {
    // Snapshot for the bevy-Resource side (Mode B mirror needs its own
    // copy with `SyncMutator` wired in; the entity-component side gets
    // the original `value`).
    let snapshot = value.copy_to_box();

    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        let result = server.insert_resource::<_, R>(world.proxy_mut(), value, is_static);
        if let Err(_e) = result {
            log::warn!(
                "naia replicate_resource: type already inserted; skipping duplicate insert"
            );
        }
    });

    let value_for_bevy: Box<R> = snapshot
        .to_boxed_any()
        .downcast::<R>()
        .expect("Box<dyn Replicate> built from R must downcast back to R");
    crate::resource_sync::install_bevy_resource_mirror::<R>(world, *value_for_bevy);
}
