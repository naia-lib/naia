
use naia_server::{EntityMut as NaiaEntityMut, EntityRef as NaiaEntityRef, ReplicationConfig, RoomKey};
use naia_shared::{AuthorityError, EntityAuthStatus, ReplicaMutWrapper, ReplicaRefWrapper, ReplicatedComponent, WorldMutType, WorldRefType};

use crate::{harness::{users::Users, entity_registry::EntityRegistry, entity_owner::EntityOwner, EntityKey}, ClientKey, TestEntity};

/// Harness wrapper for EntityRef that works with EntityKey instead of TestEntity
pub struct ServerEntityRef<'a, W: WorldRefType<TestEntity>> {
    entity_ref: NaiaEntityRef<'a, TestEntity, W>,
    users: Users<'a>,
    registry: &'a EntityRegistry,
}

impl<'a, W: WorldRefType<TestEntity>> ServerEntityRef<'a, W> {
    pub(crate) fn new(
        entity_ref: NaiaEntityRef<'a, TestEntity, W>,
        users: Users<'a>,
        registry: &'a EntityRegistry,
    ) -> Self {
        Self { entity_ref, users, registry }
    }

    /// Get the EntityKey for this entity
    pub fn key(&self) -> Option<EntityKey> {
        let entity = self.entity_ref.id();
        self.registry.entity_key_for_server_entity(&entity)
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
            naia_server::EntityOwner::Server => EntityOwner::Server,
            naia_server::EntityOwner::Client(user_key)
            | naia_server::EntityOwner::ClientWaiting(user_key)
            | naia_server::EntityOwner::ClientPublic(user_key) => {
                let client_key = self.users.user_to_client_key(&user_key)
                    .unwrap_or_else(|| panic!("UserKey {:?} not found in client map", user_key));
                EntityOwner::Client(client_key)
            }
            naia_server::EntityOwner::Local => EntityOwner::Local,
        }
    }
}

/// Harness wrapper for EntityMut that works with EntityKey instead of TestEntity
pub struct ServerEntityMut<'a, W: WorldMutType<TestEntity>> {
    entity_mut: NaiaEntityMut<'a, TestEntity, W>,
    users: Users<'a>,
    registry: &'a EntityRegistry,
}

impl<'a, W: WorldMutType<TestEntity>> ServerEntityMut<'a, W> {
    pub(crate) fn new(
        entity_mut: NaiaEntityMut<'a, TestEntity, W>,
        users: Users<'a>,
        registry: &'a EntityRegistry,
    ) -> Self {
        Self { entity_mut, users, registry }
    }

    /// Get the EntityKey for this entity
    pub fn key(&self) -> Option<EntityKey> {
        let entity = self.entity_mut.id();
        self.registry.entity_key_for_server_entity(&entity)
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

    /// Insert multiple components
    pub fn insert_components<R: ReplicatedComponent>(
        &mut self,
        component_refs: Vec<R>,
    ) -> &mut Self {
        self.entity_mut.insert_components(component_refs);
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
            naia_server::EntityOwner::Server => EntityOwner::Server,
            naia_server::EntityOwner::Client(user_key)
            | naia_server::EntityOwner::ClientWaiting(user_key)
            | naia_server::EntityOwner::ClientPublic(user_key) => {
                let client_key = self.users.user_to_client_key(&user_key)
                    .unwrap_or_else(|| panic!("UserKey {:?} not found in client map", user_key));
                EntityOwner::Client(client_key)
            }
            naia_server::EntityOwner::Local => EntityOwner::Local,
        }
    }

    /// Give authority to a user
    pub fn give_authority(&mut self, client_key: &ClientKey) -> Result<&mut Self, AuthorityError> {
        let user_key = self.users.client_to_user_key(client_key).ok_or(AuthorityError::NotInScope)?;
        self.entity_mut.give_authority(&user_key)?;
        Ok(self)
    }

    /// Take authority from the current holder
    pub fn take_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.entity_mut.take_authority()?;
        Ok(self)
    }

    /// Release authority
    pub fn release_authority(&mut self) -> Result<&mut Self, AuthorityError> {
        self.entity_mut.release_authority()?;
        Ok(self)
    }

    /// Enter a room
    pub fn enter_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.entity_mut.enter_room(room_key);
        self
    }

    /// Leave a room
    pub fn leave_room(&mut self, room_key: &RoomKey) -> &mut Self {
        self.entity_mut.leave_room(room_key);
        self
    }
}

