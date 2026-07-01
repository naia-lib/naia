use std::hash::Hash;

use naia_shared::{EntityAuthStatus, ReplicaRefWrapper, ReplicatedComponent, WorldRefType};

use crate::{server::InternalWorldServer, EntityOwner, PipelinedWorldServer, ReplicationConfig};

/// Which engine shape an [`EntityRef`] reads from (G-unify Phase 1).
///
/// Mirrors [`crate::world::entity_mut::EntityMutTarget`]: the read-only handle is
/// ONE type over both variants. The `Resident` arm reads the fused
/// [`InternalWorldServer`] directly; the `Pipelined` arm reads
/// [`PipelinedWorldServer`]'s coord-resident fast paths (replication config /
/// authority / owner are all coord-resident, so no engine reassembly is needed).
pub(crate) enum EntityRefTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s InternalWorldServer<E>),
    Pipelined(&'s PipelinedWorldServer<E>),
}

/// Scoped read-only handle for a server entity.
///
/// Obtained from [`Server::entity`] / [`crate::WorldServer::entity`]. Provides
/// read access to components, replication config, authority status, and
/// ownership without borrowing the server mutably.
pub struct EntityRef<'s, E: Copy + Eq + Hash + Send + Sync + 'static, W: WorldRefType<E>> {
    server: EntityRefTarget<'s, E>,
    world: W,
    entity: E,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static, W: WorldRefType<E>> EntityRef<'s, E, W> {
    pub(crate) fn new(server: &'s InternalWorldServer<E>, world: W, entity: &E) -> Self {
        Self::with_target(EntityRefTarget::Resident(server), world, entity)
    }

    pub(crate) fn with_target(server: EntityRefTarget<'s, E>, world: W, entity: &E) -> Self {
        Self {
            server,
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

    /// Returns the current [`ReplicationConfig`], or `None` if the entity is
    /// not registered with the replication layer.
    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        match &self.server {
            EntityRefTarget::Resident(ws) => ws.entity_replication_config(&self.entity),
            EntityRefTarget::Pipelined(ps) => ps.entity_replication_config(&self.entity),
        }
    }

    /// Returns the current authority status for this entity, or `None` if the
    /// entity is not configured as `Delegated`.
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        match &self.server {
            EntityRefTarget::Resident(ws) => ws.entity_authority_status(&self.entity),
            EntityRefTarget::Pipelined(ps) => ps.entity_authority_status(&self.entity),
        }
    }

    /// Returns the current [`EntityOwner`] — who holds authoritative control
    /// over this entity right now.
    pub fn owner(&self) -> EntityOwner {
        match &self.server {
            EntityRefTarget::Resident(ws) => ws.entity_owner(&self.entity),
            EntityRefTarget::Pipelined(ps) => ps.entity_owner(&self.entity),
        }
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        use crate::UserKey;

        impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static, W: WorldRefType<E>> EntityRef<'s, E, W> {

            /// Returns the [`LocalEntity`] id that the given user uses to
            /// identify this entity, if it is currently in their scope.
            ///
            /// Only available with the `interior_visibility` feature.
            pub fn local_entity(&self, user_key: &UserKey) -> Option<LocalEntity> {
                match &self.server {
                    EntityRefTarget::Resident(ws) => ws.world_to_local_entity(user_key, &self.entity),
                    // The user→local-entity scope map is send-resident; the
                    // pipelined arm reads it via a `&self` slot-lock read that
                    // shares the resident body (`world_to_local_entity_impl`).
                    EntityRefTarget::Pipelined(ps) => ps.world_to_local_entity(user_key, &self.entity),
                }
            }
        }
    }
}
