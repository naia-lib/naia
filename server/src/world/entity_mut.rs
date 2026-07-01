use std::hash::Hash;

use naia_shared::{
    AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicatedComponent, WorldMutType,
};

use crate::{
    room::RoomKey, server::InternalWorldServer, EntityOwner, PipelinedWorldServer,
    ReplicationConfig, UserKey,
};

/// Which engine shape an [`EntityMut`] acts on (G-unify Phase 4).
///
/// The builder is ONE type over both variants (no `PipelinedEntityMut`): the
/// `Resident` arm calls the fused `InternalWorldServer` ops directly; the
/// `Pipelined` arm uses `PipelinedWorldServer`'s coord-only fast paths where they
/// exist and otherwise reassembles the engine transiently via
/// [`PipelinedWorldServer::with_world_server`] (valid inside the park window /
/// before workers start). This keeps the imperative builder identical for
/// consumers regardless of drive shape.
pub(crate) enum EntityMutTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s mut InternalWorldServer<E>),
    Pipelined(&'s mut PipelinedWorldServer<E>),
}

/// Scoped mutable handle for a server-owned entity.
///
/// Obtained from [`Server::entity_mut`] / [`crate::WorldServer::entity_mut`].
/// Borrows the server for the duration of the handle, so only one `EntityMut`
/// can be live at a time.
///
/// # Static-entity contract
///
/// Entities may be marked *static* — replicated once in full when they enter a
/// user's scope, with no per-field diff-tracking thereafter. To create a static
/// entity:
///
/// 1. Call `as_static()` on this handle **before** inserting any components.
/// 2. Insert all components via `insert_component()` while the handle is live.
/// 3. Drop the handle.
///
/// After the handle is dropped, calling `insert_component` or
/// `remove_component` on a static entity panics. This is enforced at
/// runtime by the `allow_static_insert` flag.
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync + 'static, W: WorldMutType<E>> {
    server: EntityMutTarget<'s, E>,
    world: W,
    entity: E,
    /// True after `as_static()` is called — allows component insertion during
    /// construction. False for all other sources where mutation of a static
    /// entity after construction is illegal.
    allow_static_insert: bool,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(server: &'s mut InternalWorldServer<E>, world: W, entity: &E) -> Self {
        Self::with_target(EntityMutTarget::Resident(server), world, entity)
    }

    pub(crate) fn with_target(
        server: EntityMutTarget<'s, E>,
        world: W,
        entity: &E,
    ) -> Self {
        Self {
            server,
            world,
            entity: *entity,
            allow_static_insert: false,
        }
    }

    /// `true` if this entity is registered as static, dispatched per variant.
    fn target_entity_is_static(&self) -> bool {
        match &self.server {
            EntityMutTarget::Resident(ws) => ws.entity_is_static(&self.entity),
            EntityMutTarget::Pipelined(ps) => ps.entity_is_static(&self.entity),
        }
    }

    /// Returns the underlying entity identifier.
    pub fn id(&self) -> E {
        self.entity
    }

    /// Registers this (already world-spawned) entity with the replication
    /// layer as a server-owned entity. The imperative entry point of the
    /// builder: `server.entity_mut(e).enable_replication().configure_replication(cfg).enter_room(rk)`.
    ///
    /// # Panics
    ///
    /// Each entity may be enabled at most once (a second call would orphan the
    /// prior `GlobalEntity` mapping). Calling out of order is a deserved panic —
    /// naia keeps one imperative way to do this.
    pub fn enable_replication(&mut self) -> &mut Self {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.enable_entity_replication(&entity),
            EntityMutTarget::Pipelined(ps) => ps.enable_entity_replication(&entity),
        }
        self
    }

    /// Marks this entity as static: a full snapshot is sent once when the
    /// entity enters each user's scope; no diff-tracking occurs afterward.
    ///
    /// **Must be called before `insert_component`**; inserting components on a
    /// static entity after this handle is dropped panics.
    pub fn as_static(&mut self) -> &mut Self {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.mark_entity_as_static(&entity),
            EntityMutTarget::Pipelined(ps) => ps.mark_entity_as_static(&entity),
        }
        self.allow_static_insert = true;
        self
    }

    /// Despawns the entity, removing it from the replication layer and the
    /// ECS world. All in-scope users receive a despawn event.
    pub fn despawn(&mut self) {
        let entity = self.entity;
        let Self { server, world, .. } = self;
        match server {
            EntityMutTarget::Resident(ws) => ws.despawn_entity(world, &entity),
            EntityMutTarget::Pipelined(ps) => {
                if !world.has_entity(&entity) {
                    panic!("attempted to de-spawn nonexistent entity");
                }
                world.despawn_entity(&entity);
                ps.despawn_entity_worldless(&entity);
            }
        }
    }

    // Components

    /// Returns `true` if the entity currently carries component `R`.
    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    /// Returns a mutable accessor for component `R`, or `None` if the entity
    /// does not carry it.
    pub fn component<R: ReplicatedComponent>(&'_ mut self) -> Option<ReplicaMutWrapper<'_, R>> {
        self.world.component_mut::<R>(&self.entity)
    }

    /// Inserts component `R` onto the entity and begins diff-tracking it.
    ///
    /// # Panics
    ///
    /// Panics if the entity is static and this handle was not obtained via
    /// `as_static()` — i.e., component insertion after construction is
    /// forbidden on static entities.
    pub fn insert_component<R: ReplicatedComponent>(&mut self, component_ref: R) -> &mut Self {
        if !self.allow_static_insert && self.target_entity_is_static() {
            panic!("Cannot insert_component on a static entity after construction: call .as_static() and insert all components before dropping EntityMut");
        }
        let entity = self.entity;
        let Self { server, world, .. } = self;
        match server {
            EntityMutTarget::Resident(ws) => ws.insert_component(world, &entity, component_ref),
            EntityMutTarget::Pipelined(ps) => {
                if !world.has_entity(&entity) {
                    panic!("attempted to add component to non-existent entity");
                }

                let mut component = component_ref;
                let component_kind = component.kind();
                if world.has_component_of_kind(&entity, &component_kind) {
                    let Some(mut component_mut) = world.component_mut::<R>(&entity) else {
                        panic!("Should never happen because we checked for this above");
                    };
                    component_mut.mirror(&component);
                } else {
                    ps.insert_component_worldless(&entity, &mut component);
                    world.insert_component(&entity, component);
                }
            }
        }

        self
    }

    /// Removes component `R` from the entity, returning the value if present.
    ///
    /// # Panics
    ///
    /// Panics if called on a static entity — static entities are immutable
    /// after construction.
    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
        if self.target_entity_is_static() {
            panic!("Cannot remove_component on a static entity"); // no allow_static_insert exception — removal is never valid
        }
        let entity = self.entity;
        let Self { server, world, .. } = self;
        match server {
            EntityMutTarget::Resident(ws) => ws.remove_component::<R, W>(world, &entity),
            EntityMutTarget::Pipelined(ps) => {
                ps.remove_component_worldless(&entity, &naia_shared::ComponentKind::of::<R>());
                world.remove_component::<R>(&entity)
            }
        }
    }

    // Authority / Config

    /// Updates the [`ReplicationConfig`] for this entity (publicity + scope
    /// exit policy). Returns `&mut Self` for chaining.
    pub fn configure_replication(&mut self, config: ReplicationConfig) -> &mut Self {
        let entity = self.entity;
        let Self { server, world, .. } = self;
        match server {
            EntityMutTarget::Resident(ws) => {
                ws.configure_entity_replication(world, &entity, config)
            }
            // Pipelined: capture coord/send work without reassembly, then apply
            // world hooks immediately because this API already holds `&mut World`.
            EntityMutTarget::Pipelined(ps) => {
                ps.configure_entity_replication(&entity, config);
                ps.apply_pending_world_hooks(world);
            }
        }

        self
    }

    /// Returns the current [`ReplicationConfig`], or `None` if the entity is
    /// not registered with the replication layer.
    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        match &self.server {
            EntityMutTarget::Resident(ws) => ws.entity_replication_config(&self.entity),
            EntityMutTarget::Pipelined(ps) => ps.entity_replication_config(&self.entity),
        }
    }

    /// Returns the current authority status for this entity, or `None` if the
    /// entity is not configured as `Delegated`.
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        match &self.server {
            EntityMutTarget::Resident(ws) => ws.entity_authority_status(&self.entity),
            EntityMutTarget::Pipelined(ps) => ps.entity_authority_status(&self.entity),
        }
    }

    /// Returns the current [`EntityOwner`] — who holds authoritative control
    /// over this entity right now.
    pub fn owner(&self) -> EntityOwner {
        match &self.server {
            EntityMutTarget::Resident(ws) => ws.entity_owner(&self.entity),
            EntityMutTarget::Pipelined(ps) => ps.entity_owner(&self.entity),
        }
    }

    /// Grants authority over this entity to the specified user.
    ///
    /// The entity must be configured with `ReplicationConfig::delegated()`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the entity is not delegable, or if the
    /// authority state machine rejects the transition.
    pub fn give_authority(&mut self, user_key: &UserKey) -> Result<&mut Self, AuthorityError> {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.entity_give_authority(user_key, &entity),
            EntityMutTarget::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.entity_give_authority(user_key, &entity))
            }
        }?;
        Ok(self)
    }

    /// Reclaims server authority over this entity, revoking any client grant.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the transition is not currently valid.
    pub fn take_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.entity_take_authority(&entity),
            EntityMutTarget::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.entity_take_authority(&entity))
            }
        }?;
        Ok(self)
    }

    /// Releases authority without immediately reclaiming it (for graceful
    /// hand-back protocols). The entity transitions to the `Releasing` state.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the transition is not currently valid.
    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.entity_release_authority(None, &entity),
            EntityMutTarget::Pipelined(ps) => {
                ps.with_world_server(|ws| ws.entity_release_authority(None, &entity))
            }
        }?;
        Ok(self)
    }

    // Rooms

    /// Adds this entity to the given room, making it visible to all users in
    /// that room (subject to per-user scope checks).
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.room_add_entity(room_key, &entity),
            EntityMutTarget::Pipelined(ps) => ps.room_add_entity(room_key, &entity),
        }

        self
    }

    /// Removes this entity from the given room. Users for whom this was the
    /// only in-scope path will receive a despawn event (unless the entity's
    /// `ScopeExit` is `Persist`).
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        let entity = self.entity;
        match &mut self.server {
            EntityMutTarget::Resident(ws) => ws.room_remove_entity(room_key, &entity),
            EntityMutTarget::Pipelined(ps) => ps.room_remove_entity(room_key, &entity),
        }

        self
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {

            /// Returns the [`LocalEntity`] id that the given user uses to
            /// identify this entity, if it is currently in their scope.
            ///
            /// Only available with the `interior_visibility` feature.
            pub fn local_entity(&self, user_key: &UserKey) -> Option<LocalEntity> {
                match &self.server {
                    EntityMutTarget::Resident(ws) => {
                        ws.world_to_local_entity(user_key, &self.entity)
                    }
                    // Reads per-user send-side scope state; on a pipelined server
                    // that lives in the send worker's slot and is not reachable
                    // through a `&self` read. Not yet wired (interior_visibility
                    // is a niche feature); surface it loudly rather than silently
                    // returning None.
                    EntityMutTarget::Pipelined(_) => {
                        panic!(
                            "EntityMut::local_entity (interior_visibility) is not yet \
                             available on a pipelined WorldServer"
                        )
                    }
                }
            }
        }
    }
}
