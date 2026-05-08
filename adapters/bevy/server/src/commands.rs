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

pub trait ServerCommandsExt {
    /// Insert a Replicated Resource using the dynamic entity ID pool.
    fn replicate_resource<R: ReplicatedResource>(&mut self, value: R);

    /// Insert a Replicated Resource using the static entity ID pool —
    /// long-lived singletons; smaller wire IDs; recycled separately
    /// from gameplay entities.
    fn replicate_resource_static<R: ReplicatedResource>(&mut self, value: R);

    /// Remove the resource of type `R`. Despawns the hidden entity,
    /// propagating the removal to every client where it was in scope.
    fn remove_replicated_resource<R: ReplicatedResource>(&mut self);

    /// Configure the replication mode of resource `R` (e.g.
    /// `ReplicationConfig::delegated()` to enable client-authority
    /// requests).
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
        let result = if is_static {
            server.insert_static_resource::<_, R>(world.proxy_mut(), value)
        } else {
            server.insert_resource::<_, R>(world.proxy_mut(), value)
        };
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
