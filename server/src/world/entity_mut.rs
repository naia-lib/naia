use std::hash::Hash;

use naia_shared::{
    AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicatedComponent, WorldMutType,
};

use crate::{room::RoomKey, server::WorldServer, EntityOwner, ReplicationConfig, UserKey};

/// Scoped mutable handle for a server-owned entity.
///
/// Obtained from [`Server::entity_mut`]. Borrows `WorldServer` for the
/// duration of the handle, so only one `EntityMut` can be live at a time.
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
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    server: &'s mut WorldServer<E>,
    world: W,
    entity: E,
    /// True after `as_static()` is called — allows component insertion during
    /// construction. False for all other sources where mutation of a static
    /// entity after construction is illegal.
    allow_static_insert: bool,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(server: &'s mut WorldServer<E>, world: W, entity: &E) -> Self {
        Self {
            server,
            world,
            entity: *entity,
            allow_static_insert: false,
        }
    }

    /// Returns the underlying entity identifier.
    pub fn id(&self) -> E {
        self.entity
    }

    /// Marks this entity as static: a full snapshot is sent once when the
    /// entity enters each user's scope; no diff-tracking occurs afterward.
    ///
    /// **Must be called before `insert_component`**; inserting components on a
    /// static entity after this handle is dropped panics.
    pub fn as_static(&mut self) -> &mut Self {
        self.server.mark_entity_as_static(&self.entity);
        self.allow_static_insert = true;
        self
    }

    /// Despawns the entity, removing it from the replication layer and the
    /// ECS world. All in-scope users receive a despawn event.
    pub fn despawn(&mut self) {
        self.server.despawn_entity(&mut self.world, &self.entity);
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
        if !self.allow_static_insert && self.server.entity_is_static(&self.entity) {
            panic!("Cannot insert_component on a static entity after construction: call .as_static() and insert all components before dropping EntityMut");
        }
        self.server
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    /// Removes component `R` from the entity, returning the value if present.
    ///
    /// # Panics
    ///
    /// Panics if called on a static entity — static entities are immutable
    /// after construction.
    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
        if self.server.entity_is_static(&self.entity) {
            panic!("Cannot remove_component on a static entity"); // no allow_static_insert exception — removal is never valid
        }
        self.server
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }

    // Authority / Config

    /// Updates the [`ReplicationConfig`] for this entity (publicity + scope
    /// exit policy). Returns `&mut Self` for chaining.
    pub fn configure_replication(&mut self, config: ReplicationConfig) -> &mut Self {
        self.server
            .configure_entity_replication(&mut self.world, &self.entity, config);

        self
    }

    /// Returns the current [`ReplicationConfig`], or `None` if the entity is
    /// not registered with the replication layer.
    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.server.entity_replication_config(&self.entity)
    }

    /// Returns the current authority status for this entity, or `None` if the
    /// entity is not configured as `Delegated`.
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.server.entity_authority_status(&self.entity)
    }

    /// Returns the current [`EntityOwner`] — who holds authoritative control
    /// over this entity right now.
    pub fn owner(&self) -> EntityOwner {
        self.server.entity_owner(&self.entity)
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
        self.server.entity_give_authority(user_key, &self.entity)?;
        Ok(self)
    }

    /// Reclaims server authority over this entity, revoking any client grant.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the transition is not currently valid.
    pub fn take_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.server.entity_take_authority(&self.entity)?;
        Ok(self)
    }

    /// Releases authority without immediately reclaiming it (for graceful
    /// hand-back protocols). The entity transitions to the `Releasing` state.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the transition is not currently valid.
    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.server.entity_release_authority(None, &self.entity)?;
        Ok(self)
    }

    // Rooms

    /// Adds this entity to the given room, making it visible to all users in
    /// that room (subject to per-user scope checks).
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_add_entity(room_key, &self.entity);

        self
    }

    /// Removes this entity from the given room. Users for whom this was the
    /// only in-scope path will receive a despawn event (unless the entity's
    /// `ScopeExit` is `Persist`).
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.server.room_remove_entity(room_key, &self.entity);

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
                self.server.world_to_local_entity(user_key, &self.entity)
            }
        }
    }
}
