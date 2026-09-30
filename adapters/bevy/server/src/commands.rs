use std::ops::DerefMut;

use bevy_ecs::{
    system::{Commands, EntityCommands},
    world::Mut,
};
use naia_bevy_shared::{
    ComponentKind, EntityAuthStatus, HostOwned, Replicate, ReplicatedResource, WorldMutType,
    WorldOpCommand, WorldProxy, WorldProxyMut, WorldRefType,
};
use naia_server::{ReplicationConfig, UserKey};

use crate::{components::Replication, plugin::Singleton, server::ServerImpl, Server};

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
/// fn spawn_player(mut commands: Commands, mut server: Server) {
///     commands.spawn_empty()
///         .enable_replication(&mut server);
/// }
///
/// # #[derive(bevy_ecs::component::Component)]
/// # struct Tile;
/// fn spawn_tile(mut commands: Commands) {
///     commands.spawn_empty()
///         .as_static()
///         .insert(Tile);
/// }
/// ```
pub trait CommandsExt<'a> {
    /// Registers the entity with the naia replication layer.
    ///
    /// After this call, inserting any `#[derive(Replicate)]` component
    /// on the entity will begin diff-tracking and replication to in-scope
    /// clients. Also inserts the [`Replication`](crate::Replication) marker,
    /// so the command path and the marker path agree; inserting the marker
    /// directly is equivalent.
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a>;

    /// Marks the entity as static — no diff-tracking after initial replication.
    ///
    /// Must be called after [`enable_replication`] on the same entity.
    /// `enable_replication` registers the entity synchronously; `as_static`
    /// queues a deferred command that converts the record to static before
    /// the first game tick runs.
    ///
    /// A full component snapshot is sent once when the entity enters a user's
    /// scope. Use for tile entities, level geometry, or any entity that never
    /// mutates after spawning.
    fn as_static(&'a mut self) -> &'a mut EntityCommands<'a>;

    /// Removes the entity from the naia replication layer.
    ///
    /// Despawns the entity on all clients for whom it was in scope. Also
    /// removes the [`Replication`](crate::Replication) marker, so removing
    /// the marker directly is equivalent.
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
    /// The entity must already have `Delegated` replication config **and** must
    /// be in scope for the target user (i.e. they share a room, or an explicit
    /// `scope.include()` was called). If either precondition is not met the call
    /// is a **silent no-op** — no panic, no error event, no state change.
    ///
    /// Specifically:
    /// - Entity not found → no-op (the Bevy `EntityCommands` still refers to a
    ///   valid Bevy entity, but naia has no replication record for it).
    /// - Entity found but not `Delegated` → no-op (`AuthorityError::NotDelegated`
    ///   is returned by the inner call and discarded here).
    /// - Entity `Delegated` but not in scope for `user_key` → no-op
    ///   (`AuthorityError::NotInScope`).
    /// - All preconditions met → sends `SetAuthority(Granted)` to the target
    ///   user and `SetAuthority(Denied)` to any other user who previously held
    ///   authority.
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

    /// Stops replicating one component of a replicated entity (naia-lib/naia#186).
    ///
    /// The client removes the component; the entity and its siblings keep
    /// syncing, and the server keeps simulating the component locally.
    /// Disabling an untracked component is a silent no-op.
    fn disable_component_replication<R: Replicate>(
        &'a mut self,
        server: &mut Server,
    ) -> &'a mut EntityCommands<'a>;

    /// Re-enables replication of one component disabled with
    /// [`disable_component_replication`](CommandsExt::disable_component_replication).
    ///
    /// The client receives the component's CURRENT value, not the one it
    /// had when disabled. Enabling an already-tracked component is a
    /// silent no-op.
    fn enable_component_replication<R: Replicate>(
        &'a mut self,
        server: &mut Server,
    ) -> &'a mut EntityCommands<'a>;
}

impl<'a> CommandsExt<'a> for EntityCommands<'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        let id = self.id();
        // Converge onto the marker: the entity ends up marked whether the
        // caller used the command or the component. The guard keeps a
        // marker-then-command sequence from double-enabling (fail-loud);
        // single-call behavior is unchanged.
        if server.replication_config(&id).is_none() {
            server.enable_replication(&id);
        }
        self.insert(HostOwned::new::<Singleton>());
        self.insert(Replication);
        self
    }

    fn as_static(&'a mut self) -> &'a mut EntityCommands<'a> {
        let entity = self.id();
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|_world, mut server: Mut<ServerImpl>| {
                server.mark_entity_as_static(&entity);
            });
        }));
        self
    }

    fn disable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'a> {
        server.disable_replication(&self.id());
        self.remove::<HostOwned>();
        // Converge onto the marker: removing an absent marker is a silent
        // no-op that fires no event, so repeat disables stay quiet.
        self.remove::<Replication>();
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

    fn disable_component_replication<R: Replicate>(
        &'a mut self,
        _server: &mut Server,
    ) -> &'a mut EntityCommands<'a> {
        // Deferred: the Remove path needs world access for the authority
        // check, mirroring the `HostSyncEvent::Remove` arm of
        // `world_to_host_sync`. The Bevy component is untouched — only the
        // replication record, diff handler, and in-scope client copies go
        // away. Untracked components (and unregistered entities) converge
        // silently via the core no-op guard.
        let entity = self.id();
        let component_kind = ComponentKind::of::<R>();
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut server: Mut<ServerImpl>| {
                // The command applies after the caller's system returns; the
                // entity may have despawned in between.
                if !world.proxy().has_entity(&entity) {
                    return;
                }
                if server.entity_authority_status(world.proxy(), &entity)
                    == Some(EntityAuthStatus::Denied)
                {
                    return;
                }
                server.remove_component_worldless(&entity, &component_kind);
            });
        }));
        self
    }

    fn enable_component_replication<R: Replicate>(
        &'a mut self,
        _server: &mut Server,
    ) -> &'a mut EntityCommands<'a> {
        // Deferred: re-registration needs the component's CURRENT value from
        // the world, so the client receives a fresh Insert rather than a
        // stale snapshot. Already-tracked components (and unregistered
        // entities, or components since removed from the world) converge
        // silently — no warn, no panic, no duplicate Insert.
        let entity = self.id();
        let component_kind = ComponentKind::of::<R>();
        self.commands().queue(WorldOpCommand::new(move |world| {
            world.resource_scope(|world, mut server: Mut<ServerImpl>| {
                // The command applies after the caller's system returns; the
                // entity (or the component) may be gone by then.
                if !world.proxy().has_entity(&entity) {
                    return;
                }
                if server.entity_authority_status(world.proxy(), &entity)
                    == Some(EntityAuthStatus::Denied)
                {
                    return;
                }
                if server.has_component_record(&entity, &component_kind) {
                    return;
                }
                let mut world_proxy = world.proxy_mut();
                let Some(mut component_mut) =
                    world_proxy.component_mut_of_kind(&entity, &component_kind)
                else {
                    return;
                };
                server.insert_component_worldless(&entity, DerefMut::deref_mut(&mut component_mut));
            });
        }));
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
            let value = value_cell.lock().take().expect("WorldOpCommand runs once");
            replicate_resource_inner::<R>(world, value, /* is_static */ false);
        }));
    }

    fn replicate_resource_static<R: ReplicatedResource>(&mut self, value: R) {
        let value_cell = parking_lot::Mutex::new(Some(value));
        self.queue(WorldOpCommand::new(move |world| {
            let value = value_cell.lock().take().expect("WorldOpCommand runs once");
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
///
/// Spawns the host-owned carrier entity and inserts `value` as its `R`
/// component with the native naia `PropertyMutator` attached. Under bevy
/// 0.19 storage aliasing that carrier component IS `Res<R>`, so no
/// separate bevy-`Resource` mirror is needed: a later `ResMut<R>` field
/// write mutates the same Property cell the native mutator is attached
/// to, which records the per-field diff in naia's `DirtyQueue` and
/// replicates. Singleton-per-type is enforced by `ServerImpl::insert_resource`
/// (the resource registry rejects a duplicate kind).
fn replicate_resource_inner<R: ReplicatedResource>(
    world: &mut bevy_ecs::world::World,
    value: R,
    is_static: bool,
) {
    world.resource_scope(|world, mut server: Mut<ServerImpl>| {
        let result = server.insert_resource::<_, R>(world.proxy_mut(), value, is_static);
        if let Err(_e) = result {
            log::warn!("naia replicate_resource: type already inserted; skipping duplicate insert");
        }
    });
}
