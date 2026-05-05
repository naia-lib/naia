use std::marker::PhantomData;

use bevy_ecs::system::Command;
use bevy_ecs::{
    entity::Entity,
    system::{Commands, EntityCommands},
    world::{Mut, World},
};
use naia_bevy_shared::{EntityAuthStatus, HostOwned, ReplicatedResource, WorldProxyMut};
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

// =====================================================================
// Replicated Resources — Commands extension (D6 of RESOURCES_PLAN)
// =====================================================================
//
// User-facing API mirrors the entity-spawn split between dynamic and
// static ID pools. Each method queues a Bevy `Command` that, on
// `apply`, takes `&mut World` and uses `world.resource_scope` to
// dispatch into `ServerImpl` (same pattern as ConfigureReplicationCommand
// above).
//
// Trait lives on `Commands<'_, '_>` (not `EntityCommands`) because
// resources have no user-visible entity identity.
//
// ## Deferral semantics
//
// Per Bevy's standard `Commands` queue: calls do NOT take effect
// immediately. They queue a `Command` that runs at the next
// `apply_deferred` boundary (typically end-of-stage). This means:
//
// ```rust
// commands.replicate_resource(Score::new(0, 0));
// // server.has_resource::<Score>() returns FALSE here — command queued, not applied
// ```
//
// To observe the resource within the same system, schedule a
// follow-up system after an `apply_deferred` flush, or use
// `world.resource_scope` directly in an exclusive system. This is
// standard Bevy behavior; the same caveat applies to
// `commands.spawn(...)` / `commands.insert_resource(...)`.

pub trait ServerCommandsExt {
    /// Insert a Replicated Resource using the dynamic entity ID pool.
    /// Equivalent to a server `commands.spawn(...).enable_replication(...)`
    /// on a hidden 1-component entity.
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
    fn configure_replicated_resource<R: ReplicatedResource>(
        &mut self,
        config: ReplicationConfig,
    );
}

impl<'w, 's> ServerCommandsExt for Commands<'w, 's> {
    fn replicate_resource<R: ReplicatedResource>(&mut self, value: R) {
        self.queue(ReplicateResourceCommand::<R>::new_dynamic(value));
    }

    fn replicate_resource_static<R: ReplicatedResource>(&mut self, value: R) {
        self.queue(ReplicateResourceCommand::<R>::new_static(value));
    }

    fn remove_replicated_resource<R: ReplicatedResource>(&mut self) {
        self.queue(RemoveReplicatedResourceCommand::<R>::new());
    }

    fn configure_replicated_resource<R: ReplicatedResource>(
        &mut self,
        config: ReplicationConfig,
    ) {
        self.queue(ConfigureReplicatedResourceCommand::<R>::new(config));
    }
}

//// ReplicateResourceCommand ////
pub(crate) struct ReplicateResourceCommand<R: ReplicatedResource> {
    value: Option<R>,
    is_static: bool,
}

impl<R: ReplicatedResource> ReplicateResourceCommand<R> {
    pub fn new_dynamic(value: R) -> Self {
        Self { value: Some(value), is_static: false }
    }
    pub fn new_static(value: R) -> Self {
        Self { value: Some(value), is_static: true }
    }
}

impl<R: ReplicatedResource> Command for ReplicateResourceCommand<R> {
    fn apply(mut self, world: &mut World) {
        let value = self.value.take().expect("value present at command construction");
        let is_static = self.is_static;

        // Replicated Resources surface as standard Bevy `Res<R>` /
        // `ResMut<R>` in user systems. Two storage locations are kept
        // in sync via the SyncMutator + per-tick sync system:
        //   - bevy `Resource<R>` — user-facing read/write surface
        //   - entity-component `R` on the hidden resource entity —
        //     wire-replication surface
        //
        // The bevy-resource side has its Property mutators wired to
        // a `SyncMutator<R>` so `*resmut.field = v` records the field
        // index in `SyncDirtyTracker<R>`; the per-tick sync system
        // calls `mirror_single_field` on the entity-component for each
        // touched index. End-to-end per-field diff preserved.
        //
        // Snapshot (a clone) goes to the bevy-Resource side; the
        // original `value` goes to the entity-component side. The
        // entity-component side gets the standard naia
        // PropertyMutator (wired by `WorldServer::insert_resource`).
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

        // Bevy-Resource mirror (Mode B). Panics if the user forgot to
        // call `add_resource_events::<R>()` first — that's the
        // canonical registration entry point and missing it is a
        // user error, not a degraded mode.
        let value_for_bevy: Box<R> = snapshot
            .to_boxed_any()
            .downcast::<R>()
            .expect("Box<dyn Replicate> built from R must downcast back to R");
        crate::resource_sync::install_bevy_resource_mirror::<R>(world, *value_for_bevy);
    }
}

//// RemoveReplicatedResourceCommand ////
pub(crate) struct RemoveReplicatedResourceCommand<R: ReplicatedResource> {
    _phantom: PhantomData<R>,
}

impl<R: ReplicatedResource> RemoveReplicatedResourceCommand<R> {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
}

impl<R: ReplicatedResource> Command for RemoveReplicatedResourceCommand<R> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            let _ = server.remove_resource::<_, R>(world.proxy_mut());
        });
    }
}

//// ConfigureReplicatedResourceCommand ////
pub(crate) struct ConfigureReplicatedResourceCommand<R: ReplicatedResource> {
    config: ReplicationConfig,
    _phantom: PhantomData<R>,
}

impl<R: ReplicatedResource> ConfigureReplicatedResourceCommand<R> {
    pub fn new(config: ReplicationConfig) -> Self {
        Self { config, _phantom: PhantomData }
    }
}

impl<R: ReplicatedResource> Command for ConfigureReplicatedResourceCommand<R> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            let _ = server.configure_resource::<_, R>(&mut world.proxy_mut(), self.config);
        });
    }
}
