use std::hash::Hash;

use naia_shared::{EntityAuthStatus, ReplicaRefWrapper, ReplicatedComponent, WorldRefType};

use crate::{world::entity_owner::EntityOwner, Client};
use naia_shared::Publicity;

/// Scoped read-only handle for a client entity.
///
/// Obtained from [`Client::entity`]. Provides read access to components,
/// replication config, authority status, and ownership without borrowing the
/// client mutably.
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> {
    client: &'s Client<E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub fn new(client: &'s Client<E>, world: W, entity: &E) -> Self {
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

    /// Returns `true` if the entity currently carries component `R`.
    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.world.has_component::<R>(&self.entity)
    }

    /// Returns a read-only accessor for component `R`, or `None` if the
    /// entity does not carry it.
    pub fn component<R: ReplicatedComponent>(&'_ self) -> Option<ReplicaRefWrapper<'_, R>> {
        self.world.component::<R>(&self.entity)
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
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<'s, E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>> EntityRef<'s, E, W> {

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
