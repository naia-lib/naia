use naia_client::ReplicationConfig;
use naia_shared::WorldMutType;

use crate::{TestEntity, Position};
use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Context for client-side actions
pub struct ClientMutateCtx<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
}

impl<'a> ClientMutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, client_key: ClientKey) -> Self {
        Self {
            scenario,
            client_key,
        }
    }

    /// Begin spawning a new entity
    pub fn spawn(&mut self) -> ClientSpawnBuilder<'_> {
        ClientSpawnBuilder::new(self.scenario, self.client_key)
    }

    /// Get a mutable view of an entity
    pub fn entity(&mut self, entity: EntityKey) -> ClientEntityMut<'_> {
        ClientEntityMut::new(self.scenario, self.client_key, entity)
    }
}

/// Builder for spawning entities with components
pub struct ClientSpawnBuilder<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
    position: Option<Position>,
}

impl<'a> ClientSpawnBuilder<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, client_key: ClientKey) -> Self {
        Self {
            scenario,
            client_key,
            position: None,
        }
    }

    /// Attach a Position component to the entity
    pub fn with_position(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    /// Finalize the spawn and return the EntityKey
    pub fn track(self) -> EntityKey {
        let state = self.scenario.client_state_mut(self.client_key);
        let mut entity_mut = state
            .client
            .spawn_entity(state.world.proxy_mut());

        if let Some(pos) = self.position {
            entity_mut.insert_component(pos);
        }

        // Configure entity as Public so it can be replicated to other clients
        // This must be done on the client side, not the server side
        entity_mut.configure_replication(ReplicationConfig::Public);

        let entity = entity_mut.id();

        // Allocate EntityKey and register in registry
        let entity_key = self.scenario.entity_registry_mut().allocate_entity_key();
        self.scenario
            .entity_registry_mut()
            .register_spawning_client(entity_key, self.client_key, entity);

        entity_key
    }
}

/// Mutable view of an entity on a client
pub struct ClientEntityMut<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
    entity_key: EntityKey,
}

impl<'a> ClientEntityMut<'a> {
    pub(crate) fn new(
        scenario: &'a mut Scenario,
        client_key: ClientKey,
        entity_key: EntityKey,
    ) -> Self {
        Self {
            scenario,
            client_key,
            entity_key,
        }
    }

    /// Get the client-side entity ID
    fn get_entity(&self) -> TestEntity {
        self.scenario
            .entity_registry()
            .get_client_entity(self.entity_key, self.client_key)
            .expect("EntityKey not mapped to client entity")
    }

    /// Configure replication to use delegated/authority-based replication
    pub fn delegate(self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        let mut world_mut = state.world.proxy_mut();
        state
            .client
            .configure_entity_replication(&mut world_mut, &entity, ReplicationConfig::Delegated);
    }

    /// Request authority over this entity
    pub fn request_auth(self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        state.client.entity_request_authority(&entity);
    }

    /// Release authority over this entity
    pub fn release_auth(self) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        state.client.entity_release_authority(&entity);
    }

    /// Set/update the position of the entity
    pub fn set_position(self, position: Position) {
        let entity = self.get_entity();
        let state = self.scenario.client_state_mut(self.client_key);
        
        // Use EntityMut which handles both insert and update
        let mut world_mut = state.world.proxy_mut();
        let mut entity_mut = state.client.entity_mut(world_mut, &entity);
        entity_mut.insert_component(position);
    }
}

