use std::hash::Hash;

use naia_shared::{
    AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicatedComponent, WorldMutType,
};

use crate::{world::entity_owner::EntityOwner, Client};
use naia_shared::Publicity;

/// Scoped mutable handle for a client-owned entity.
///
/// Obtained from [`Client::entity_mut`]. Borrows `Client` for the duration of
/// the handle, so only one `EntityMut` can be live at a time.
///
/// # Static-entity contract
///
/// Entities may be marked *static* — replicated once in full when they enter
/// the server's scope, with no per-field diff-tracking thereafter. To create
/// a static entity:
///
/// 1. Call `as_static()` on this handle **before** inserting any components.
/// 2. Insert all components via `insert_component()` while the handle is live.
/// 3. Drop the handle.
///
/// After the handle is dropped, calling `insert_component` or
/// `remove_component` on a static entity panics. This is enforced at
/// runtime by the `allow_static_insert` flag.
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    client: &'s mut Client<E>,
    world: W,
    entity: E,
    /// True after `as_static()` is called — allows component insertion during
    /// construction. False for all other sources where mutation of a static
    /// entity after construction is illegal.
    pub(crate) allow_static_insert: bool,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(client: &'s mut Client<E>, world: W, entity: &E) -> Self {
        Self {
            client,
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
    /// entity enters the server's scope; no diff-tracking occurs afterward.
    ///
    /// **Must be called before `insert_component`**; inserting components on a
    /// static entity after this handle is dropped panics.
    ///
    /// Safe to call before the server connection is established. If called
    /// after the entity is already connected and in the dynamic ID pool,
    /// prefer [`Client::spawn_static_entity`] instead.
    pub fn as_static(&mut self) -> &mut Self {
        self.client.mark_entity_as_static(&self.entity);
        self.allow_static_insert = true;
        self
    }

    /// Despawns the entity locally and removes it from the replication layer.
    pub fn despawn(&mut self) {
        self.client.despawn_entity(&mut self.world, &self.entity);
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

    /// Inserts component `R` onto the entity and registers it for replication.
    ///
    /// # Panics
    ///
    /// Panics if the entity is static and this handle was not obtained via
    /// `as_static()` or `spawn_static_entity()`.
    pub fn insert_component<R: ReplicatedComponent>(&mut self, component_ref: R) -> &mut Self {
        if !self.allow_static_insert && self.client.entity_is_static(&self.entity) {
            panic!("Cannot insert_component on a static entity after construction: call .as_static() and insert all components before dropping EntityMut");
        }
        self.client
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    /// Removes component `R` from the entity, returning its value if present.
    ///
    /// # Panics
    ///
    /// Panics if called on a static entity — static entities are immutable
    /// after construction.
    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
        if self.client.entity_is_static(&self.entity) {
            panic!("Cannot remove_component on a static entity");
        }
        self.client
            .remove_component::<R, W>(&mut self.world, &self.entity)
    }

    // Authority / Config

    /// Updates the [`Publicity`] for this entity. Returns `&mut Self` for
    /// chaining.
    pub fn configure_replication(&mut self, config: Publicity) -> &mut Self {
        self.client
            .configure_entity_replication(&mut self.world, &self.entity, config);

        self
    }

    /// Returns the current [`Publicity`], or `None` if the entity is not
    /// registered with the replication layer.
    pub fn replication_config(&self) -> Option<Publicity> {
        self.client.entity_replication_config(&self.entity)
    }

    /// Returns the current authority status for this entity, or `None` if the
    /// entity is not configured as `Delegated`.
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.client.entity_authority_status(&self.entity)
    }

    /// Returns the current [`EntityOwner`] — who holds authoritative control
    /// over this entity right now.
    pub fn owner(&self) -> EntityOwner {
        self.client.entity_owner(&self.entity)
    }

    /// Sends an authority request to the server for this delegated entity.
    ///
    /// The server responds asynchronously with an `EntityAuthGrantedEvent` or
    /// `EntityAuthDeniedEvent`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the entity is not in a requestable state.
    pub fn request_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.client.entity_request_authority(&self.entity)?;
        Ok(self)
    }

    /// Releases the client's authority over this entity back to the server.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError`] if the entity is not currently
    /// client-authoritative.
    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.client.entity_release_authority(&self.entity)?;
        Ok(self)
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {

            /// Returns the [`LocalEntity`] id the server assigned to this
            /// entity, if it is currently in scope.
            ///
            /// Only available with the `interior_visibility` feature.
            pub fn local_entity(&self) -> Option<LocalEntity> {
                self.client.world_to_local_entity(&self.entity)
            }
        }
    }
}
