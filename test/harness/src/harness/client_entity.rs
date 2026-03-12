use naia_client::{EntityMut as NaiaEntityMut, EntityRef as NaiaEntityRef, ReplicationConfig};
use naia_shared::{
    AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicaRefWrapper, ReplicatedComponent,
    WorldMutType, WorldRefType,
};

use crate::harness::{
    entity_owner::EntityOwner, entity_registry::EntityRegistry, ClientKey, EntityKey,
};
use crate::TestEntity;

/// Harness wrapper for client EntityRef that works with EntityKey instead of TestEntity
pub struct ClientEntityRef<'a, W: WorldRefType<TestEntity>> {
    entity_ref: NaiaEntityRef<'a, TestEntity, W>,
    registry: &'a EntityRegistry,
    client_key: ClientKey,
}

impl<'a, W: WorldRefType<TestEntity>> ClientEntityRef<'a, W> {
    pub(crate) fn new(
        entity_ref: NaiaEntityRef<'a, TestEntity, W>,
        registry: &'a EntityRegistry,
        client_key: ClientKey,
    ) -> Self {
        Self {
            entity_ref,
            registry,
            client_key,
        }
    }

    /// Get the EntityKey for this entity
    pub fn key(&self) -> Option<EntityKey> {
        let entity = self.entity_ref.id();
        self.registry
            .entity_key_for_client_test_entity(&self.client_key, &entity)
    }

    /// Get the underlying TestEntity id
    pub fn id(&self) -> TestEntity {
        self.entity_ref.id()
    }

    /// Check if this entity has a component
    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.entity_ref.has_component::<R>()
    }

    /// Get a component reference
    pub fn component<R: ReplicatedComponent>(&'_ self) -> Option<ReplicaRefWrapper<'_, R>> {
        self.entity_ref.component::<R>()
    }

    /// Get the replication configuration
    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.entity_ref.replication_config()
    }

    /// Get the authority status
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.entity_ref.authority()
    }

    /// Get the entity owner
    pub fn owner(&self) -> EntityOwner {
        match self.entity_ref.owner() {
            naia_client::EntityOwner::Server => EntityOwner::Server,
            naia_client::EntityOwner::Client => EntityOwner::Client(self.client_key),
            naia_client::EntityOwner::Local => EntityOwner::Local,
        }
    }
}

/// Harness wrapper for client EntityMut that works with EntityKey instead of TestEntity
pub struct ClientEntityMut<'a, W: WorldMutType<TestEntity>> {
    entity_mut: NaiaEntityMut<'a, TestEntity, W>,
    registry: &'a EntityRegistry,
    client_key: ClientKey,
}

impl<'a, W: WorldMutType<TestEntity>> ClientEntityMut<'a, W> {
    pub(crate) fn new(
        entity_mut: NaiaEntityMut<'a, TestEntity, W>,
        registry: &'a EntityRegistry,
        client_key: ClientKey,
    ) -> Self {
        Self {
            entity_mut,
            registry,
            client_key,
        }
    }

    /// Get the EntityKey for this entity
    pub fn key(&self) -> Option<EntityKey> {
        let entity = self.entity_mut.id();
        self.registry
            .entity_key_for_client_test_entity(&self.client_key, &entity)
    }

    /// Get the underlying TestEntity id
    pub fn id(&self) -> TestEntity {
        self.entity_mut.id()
    }

    /// Despawn this entity
    pub fn despawn(&mut self) {
        self.entity_mut.despawn();
    }

    /// Check if this entity has a component
    pub fn has_component<R: ReplicatedComponent>(&self) -> bool {
        self.entity_mut.has_component::<R>()
    }

    /// Get a mutable component reference
    pub fn component<R: ReplicatedComponent>(&'_ mut self) -> Option<ReplicaMutWrapper<'_, R>> {
        self.entity_mut.component::<R>()
    }

    /// Insert a component
    pub fn insert_component<R: ReplicatedComponent>(&mut self, component_ref: R) -> &mut Self {
        self.entity_mut.insert_component(component_ref);
        self
    }

    /// Remove a component
    pub fn remove_component<R: ReplicatedComponent>(&mut self) -> Option<R> {
        self.entity_mut.remove_component::<R>()
    }

    /// Configure replication
    pub fn configure_replication(&mut self, config: ReplicationConfig) -> &mut Self {
        self.entity_mut.configure_replication(config);
        self
    }

    /// Get the replication configuration
    pub fn replication_config(&self) -> Option<ReplicationConfig> {
        self.entity_mut.replication_config()
    }

    /// Get the authority status
    pub fn authority(&self) -> Option<EntityAuthStatus> {
        self.entity_mut.authority()
    }

    /// Get the entity owner
    pub fn owner(&self) -> EntityOwner {
        match self.entity_mut.owner() {
            naia_client::EntityOwner::Server => EntityOwner::Server,
            naia_client::EntityOwner::Client => EntityOwner::Client(self.client_key),
            naia_client::EntityOwner::Local => EntityOwner::Local,
        }
    }

    /// Request authority for this entity
    pub fn request_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.entity_mut.request_authority()?;
        Ok(self)
    }

    /// Release authority for this entity
    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.entity_mut.release_authority()?;
        Ok(self)
    }
}
