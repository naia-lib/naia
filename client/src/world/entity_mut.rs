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
/// Unlike the server counterpart, the client has no static-entity concept —
/// components may be inserted or removed freely while the client holds
/// authority over the entity.
pub struct EntityMut<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> {
    client: &'s mut Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>> EntityMut<'s, E, W> {
    pub(crate) fn new(client: &'s mut Client<E>, world: W, entity: &E) -> Self {
        Self {
            client,
            world,
            entity: *entity,
        }
    }

    /// Returns the underlying entity identifier.
    pub fn id(&self) -> E {
        self.entity
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
    pub fn insert_component<R: ReplicatedComponent>(&mut self, component_ref: R) -> &mut Self {
        self.client
            .insert_component(&mut self.world, &self.entity, component_ref);

        self
    }

    /// Removes component `R` from the entity, returning its value if present.
    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
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
