use std::marker::PhantomData;

use bevy_ecs::system::Command;
use bevy_ecs::{
    component::Mutable,
    entity::Entity,
    system::{Commands, EntityCommands},
    world::{Mut, World},
};
use naia_bevy_shared::{EntityAuthStatus, HostOwned, Replicate, WorldProxyMut};
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

/// Type alias capturing the bound a Replicated Resource type must
/// satisfy: `Replicate` + a Bevy `Component` whose mutability is
/// `Mutable`. Resource values are stored as components on the hidden
/// resource entity, so the same bound as `ReplicatedComponent` applies.
trait ResourceBound: Replicate + bevy_ecs::component::Component<Mutability = Mutable> {}
impl<T: Replicate + bevy_ecs::component::Component<Mutability = Mutable>> ResourceBound for T {}

pub trait CommandsExtServer {
    /// Insert a Replicated Resource using the dynamic entity ID pool.
    /// Equivalent to a server `commands.spawn(...).enable_replication(...)`
    /// on a hidden 1-component entity.
    fn replicate_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource + bevy_ecs::resource::Resource>(&mut self, value: R);

    /// Insert a Replicated Resource using the static entity ID pool —
    /// long-lived singletons; smaller wire IDs; recycled separately
    /// from gameplay entities.
    fn replicate_resource_static<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource + bevy_ecs::resource::Resource>(&mut self, value: R);

    /// Remove the resource of type `R`. Despawns the hidden entity,
    /// propagating the removal to every client where it was in scope.
    fn remove_replicated_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource + bevy_ecs::resource::Resource>(&mut self);

    /// Configure the replication mode of resource `R` (e.g.
    /// `ReplicationConfig::delegated()` to enable client-authority
    /// requests).
    fn configure_replicated_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource + bevy_ecs::resource::Resource>(
        &mut self,
        config: ReplicationConfig,
    );
}

impl<'w, 's> CommandsExtServer for Commands<'w, 's> {
    fn replicate_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource>(&mut self, value: R) {
        self.queue(ReplicateResourceCommand::<R>::new_dynamic(value));
    }

    fn replicate_resource_static<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource>(&mut self, value: R) {
        self.queue(ReplicateResourceCommand::<R>::new_static(value));
    }

    fn remove_replicated_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource>(&mut self) {
        self.queue(RemoveReplicatedResourceCommand::<R>::new());
    }

    fn configure_replicated_resource<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource>(
        &mut self,
        config: ReplicationConfig,
    ) {
        self.queue(ConfigureReplicatedResourceCommand::<R>::new(config));
    }
}

//// ReplicateResourceCommand ////
pub(crate) struct ReplicateResourceCommand<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> {
    value: Option<R>,
    is_static: bool,
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> ReplicateResourceCommand<R> {
    pub fn new_dynamic(value: R) -> Self {
        Self { value: Some(value), is_static: false }
    }
    pub fn new_static(value: R) -> Self {
        Self { value: Some(value), is_static: true }
    }
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> Command for ReplicateResourceCommand<R> {
    fn apply(mut self, world: &mut World) {
        let value = self.value.take().expect("value present at command construction");
        let is_static = self.is_static;

        // Mode B: also store R as a bevy Resource so `Res<R>`/`ResMut<R>`
        // works in user systems. The bevy-resource side gets a SyncMutator
        // wired in, the entity-component side gets the standard naia
        // mutator (set up by WorldServer::insert_resource). The per-tick
        // sync system mirrors dirty fields from bevy-resource → entity-
        // component using `Replicate::mirror_single_field`. See
        // `resource_sync.rs` for the full architecture.
        //
        // We only install the bevy-Resource side if R: bevy::Resource —
        // detected by attempting to insert the SyncDirtyTracker. The R
        // bound on this Command requires Replicate + Component; the
        // additional bevy::Resource bound is enforced at registration
        // time via `add_resource_events::<R>()` (Mode B path) but we
        // don't require it here so users who want Mode A only (no bevy
        // Resource, just Query<&R>) are still supported.

        // First, do the entity-component insert (always).
        // We clone `value` for the bevy-Resource side IF the type has
        // the Resource bound — but without specialization, we can't
        // detect that here. So: always clone (cheap for typical
        // resources), insert the entity-component first, then if a
        // SyncDirtyTracker<R> is present (= add_resource_events ran),
        // also insert the bevy Resource with a SyncMutator wired in.
        // If no tracker, skip the bevy-Resource side — Mode A only.
        let snapshot_for_bevy = clone_via_replicate::<R>(&value);

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

        // Mode B install: if the user registered R via add_resource_events
        // (which inserts a SyncDirtyTracker<R> Resource and the sync
        // system), wire the bevy-resource side now. Done as a separate
        // step so Mode A users (no add_resource_events Mode B install)
        // skip it cleanly.
        crate::resource_sync::install_bevy_resource_mirror_if_present::<R>(
            world,
            snapshot_for_bevy,
        );
    }
}

/// Clone a Replicate via the trait's own clone path. Avoids requiring
/// `R: Clone` on the Command bound while still allowing Mode B to keep
/// a bevy-side mirror.
fn clone_via_replicate<R: Replicate>(value: &R) -> Box<dyn Replicate> {
    value.copy_to_box()
}

//// RemoveReplicatedResourceCommand ////
pub(crate) struct RemoveReplicatedResourceCommand<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> {
    _phantom: PhantomData<R>,
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> RemoveReplicatedResourceCommand<R> {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> Command for RemoveReplicatedResourceCommand<R> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            let _ = server.remove_resource::<_, R>(world.proxy_mut());
        });
    }
}

//// ConfigureReplicatedResourceCommand ////
pub(crate) struct ConfigureReplicatedResourceCommand<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> {
    config: ReplicationConfig,
    _phantom: PhantomData<R>,
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> ConfigureReplicatedResourceCommand<R> {
    pub fn new(config: ReplicationConfig) -> Self {
        Self { config, _phantom: PhantomData }
    }
}

impl<R: Replicate + bevy_ecs::component::Component<Mutability = Mutable> + bevy_ecs::resource::Resource> Command for ConfigureReplicatedResourceCommand<R> {
    fn apply(self, world: &mut World) {
        world.resource_scope(|world, mut server: Mut<ServerImpl>| {
            let _ = server.configure_resource::<_, R>(&mut world.proxy_mut(), self.config);
        });
    }
}
