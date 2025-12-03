use naia_client::ReplicationConfig;
use naia_shared::{EntityAuthStatus, WorldRefType, WorldMutType};

use crate::{TestEntity, TestWorld, Position};
use crate::helpers::update_all_at;

use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Context for performing actions in a mutate phase
pub struct CtxMutate<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> CtxMutate<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    /// Perform server-side actions
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerCtxMutate) -> R) -> R {
        let mut ctx = ServerCtxMutate::new(self.scenario);
        f(&mut ctx)
    }

    /// Perform client-side actions
    pub fn client<R>(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientCtxMutate) -> R) -> R {
        let mut ctx = ClientCtxMutate::new(self.scenario, client);
        f(&mut ctx)
    }
}

/// Context for server-side actions
pub struct ServerCtxMutate<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> ServerCtxMutate<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    /// Include an entity in scope for a client
    pub fn include_in_scope(&mut self, client: ClientKey, entity: EntityKey) {
        let server_entity = self
            .scenario
            .entity_registry()
            .get_server_entity(entity)
            .expect("EntityKey not mapped to server entity");

        let user_key = self.scenario.user_key(client);
        self.scenario
            .server_mut()
            .user_scope_mut(&user_key)
            .include(&server_entity);
    }
}

/// Context for client-side actions
pub struct ClientCtxMutate<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
}

impl<'a> ClientCtxMutate<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, client_key: ClientKey) -> Self {
        Self {
            scenario,
            client_key,
        }
    }

    /// Begin spawning a new entity
    pub fn spawn(&mut self) -> SpawnBuilder<'_> {
        SpawnBuilder::new(self.scenario, self.client_key)
    }

    /// Get a mutable view of an entity
    pub fn entity(&mut self, entity: EntityKey) -> ClientEntityMut<'_> {
        ClientEntityMut::new(self.scenario, self.client_key, entity)
    }
}

/// Builder for spawning entities with components
pub struct SpawnBuilder<'a> {
    scenario: &'a mut Scenario,
    client_key: ClientKey,
    position: Option<Position>,
}

impl<'a> SpawnBuilder<'a> {
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

