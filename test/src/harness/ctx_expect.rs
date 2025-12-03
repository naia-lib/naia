use std::any::TypeId;

use naia_shared::{EntityAuthStatus, WorldRefType};
use naia_server::DelegateEntityEvent;

use crate::{TestEntity, Position};

use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// A predicate that can be evaluated each tick
#[derive(Clone, Debug)]
enum Expectation {
    ServerHasEntity(EntityKey),
    ServerEvent(TypeId, String), // event type and label
    ClientSeesEntity(ClientKey, EntityKey),
    ClientReplicationIsDelegated(ClientKey, EntityKey),
    ClientAuthIs(ClientKey, EntityKey, EntityAuthStatus),
    ClientPositionIs(ClientKey, EntityKey, f32, f32),
}

/// Context for registering expectations in an expect phase
pub struct ExpectCtx<'a> {
    scenario: &'a mut Scenario,
    expectations: Vec<Expectation>,
    max_ticks: usize,
}

impl<'a> ExpectCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, max_ticks: usize) -> Self {
        Self {
            scenario,
            expectations: Vec::new(),
            max_ticks,
        }
    }

    /// Override the default maximum tick budget
    pub fn ticks(&mut self, max_ticks: usize) {
        self.max_ticks = max_ticks;
    }

    /// Register server-side expectations
    pub fn server(&mut self, f: impl FnOnce(&mut ServerExpectCtx<'_, 'a>)) {
        let mut ctx = ServerExpectCtx { expect_ctx: self };
        f(&mut ctx);
    }

    /// Register client-side expectations
    pub fn client(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientExpectCtx<'_, 'a>)) {
        let mut ctx = ClientExpectCtx {
            expect_ctx: self,
            client_key: client,
        };
        f(&mut ctx);
    }

    /// Evaluate all expectations and return true if all pass
    fn evaluate_all(&mut self) -> bool {
        for expectation in &self.expectations.clone() {
            if !self.evaluate_expectation(expectation) {
                return false;
            }
        }
        true
    }

    fn evaluate_expectation(&mut self, expectation: &Expectation) -> bool {
        match expectation {
            Expectation::ServerHasEntity(entity_key) => {
                if self.scenario.entity_registry().has_server_entity(*entity_key) {
                    true
                } else {
                    self.auto_discover_server_entity(*entity_key)
                }
            }
            Expectation::ServerEvent(type_id, _label) => {
                let mut events = self.scenario.take_server_events();
                // Check if any event of this type exists
                // For now, just check for DelegateEntityEvent
                if *type_id == TypeId::of::<DelegateEntityEvent>() {
                    let mut found = false;
                    for (_user_key, _entity) in events.read::<DelegateEntityEvent>() {
                        found = true;
                        break;
                    }
                    found
                } else {
                    false
                }
            }
            Expectation::ClientSeesEntity(client_key, entity_key) => {
                if self
                    .scenario
                    .entity_registry()
                    .has_client_entity(*entity_key, *client_key)
                {
                    true
                } else {
                    self.auto_discover_client_entity(*client_key, *entity_key)
                }
            }
            Expectation::ClientReplicationIsDelegated(client_key, entity_key) => {
                if let Some(entity) = self
                    .scenario
                    .entity_registry()
                    .get_client_entity(*entity_key, *client_key)
                {
                    let state = self.scenario.client_state_mut(*client_key);
                    state
                        .client
                        .entity_replication_config(&entity)
                        .map(|config| config.is_delegated())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            Expectation::ClientAuthIs(client_key, entity_key, expected) => {
                if let Some(entity) = self
                    .scenario
                    .entity_registry()
                    .get_client_entity(*entity_key, *client_key)
                {
                    let state = self.scenario.client_state_mut(*client_key);
                    state.client.entity_authority_status(&entity) == Some(*expected)
                } else {
                    false
                }
            }
            Expectation::ClientPositionIs(client_key, entity_key, expected_x, expected_y) => {
                if let Some(entity) = self
                    .scenario
                    .entity_registry()
                    .get_client_entity(*entity_key, *client_key)
                {
                    let state = self.scenario.client_state_mut(*client_key);
                    if let Some(pos) = state.world.proxy().component::<Position>(&entity) {
                        (*pos.x - expected_x).abs() < 0.001 && (*pos.y - expected_y).abs() < 0.001
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }

    fn auto_discover_server_entity(&mut self, entity_key: EntityKey) -> bool {
        // Simple heuristic: use first entity if registry doesn't have mapping yet
        let entities = self.scenario.server_world_mut().proxy().entities();
        if !entities.is_empty() && !self.scenario.entity_registry().has_server_entity(entity_key) {
            // Map first entity to this key
            // Note: This is a simple heuristic; in real tests, you might want more sophisticated matching
            let first_entity = entities[0];
            self.scenario
                .entity_registry_mut()
                .map_server_entity(entity_key, first_entity);
            true
        } else {
            false
        }
    }

    fn auto_discover_client_entity(&mut self, client_key: ClientKey, entity_key: EntityKey) -> bool {
        let state = self.scenario.client_state_mut(client_key);
        let entities = state.world.proxy().entities();
        if !entities.is_empty()
            && !self
                .scenario
                .entity_registry()
                .has_client_entity(entity_key, client_key)
        {
            // Map first entity to this key
            let first_entity = entities[0];
            self.scenario
                .entity_registry_mut()
                .map_client_entity(entity_key, client_key, first_entity);
            true
        } else {
            false
        }
    }

    pub(crate) fn add_expectation(&mut self, expectation: Expectation) {
        self.expectations.push(expectation);
    }

    pub(crate) fn run(&mut self) {
        for tick in 0..self.max_ticks {
            self.scenario.tick_once();

            // Auto-discover entities before evaluating
            self.auto_discover_all_entities();

            if self.evaluate_all() {
                return;
            }

            if tick == self.max_ticks - 1 {
                // Timeout - panic with descriptive error
                let mut failed = Vec::new();
                let expectations_clone = self.expectations.clone();
                for expectation in &expectations_clone {
                    if !self.evaluate_expectation(expectation) {
                        failed.push(format!("{:?}", expectation));
                    }
                }
                panic!(
                    "Expect phase timed out after {} ticks. Failed expectations: {:?}",
                    self.max_ticks, failed
                );
            }
        }
    }

    fn auto_discover_all_entities(&mut self) {
        // Discover server entities
        let entities = self.scenario.server_world_mut().proxy().entities();
        let expectations_clone = self.expectations.clone();
        for expectation in expectations_clone {
            if let Expectation::ServerHasEntity(entity_key) = expectation {
                if !self.scenario.entity_registry().has_server_entity(entity_key)
                    && !entities.is_empty()
                {
                    // Use first entity that isn't already mapped to another key
                    // Simple heuristic: if only one entity, use it; otherwise use first unmapped one
                    let mut candidate = None;
                    for entity in &entities {
                        // Check if this entity is already mapped to a different key
                        if !self.scenario.entity_registry().is_server_entity_mapped(*entity) {
                            candidate = Some(*entity);
                            break;
                        }
                    }
                    // If all entities are mapped, just use the first one (fallback)
                    let entity_to_map = candidate.unwrap_or(entities[0]);
                    self.scenario
                        .entity_registry_mut()
                        .map_server_entity(entity_key, entity_to_map);
                }
            }
        }

        // Discover client entities
        let expectations_clone = self.expectations.clone();
        for expectation in expectations_clone {
            match expectation {
                Expectation::ClientSeesEntity(client_key, entity_key)
                | Expectation::ClientReplicationIsDelegated(client_key, entity_key)
                | Expectation::ClientAuthIs(client_key, entity_key, _)
                | Expectation::ClientPositionIs(client_key, entity_key, _, _) => {
                    if !self
                        .scenario
                        .entity_registry()
                        .has_client_entity(entity_key, client_key)
                    {
                        let state = self.scenario.client_state_mut(client_key);
                        let entities = state.world.proxy().entities();
                        if !entities.is_empty() {
                            // Use first entity that isn't already mapped to another key for this client
                            let mut candidate = None;
                            for entity in &entities {
                                // Check if this entity is already mapped to a different key for this client
                                if !self.scenario.entity_registry()
                                    .is_client_entity_mapped_to_different_key(*entity, client_key, entity_key)
                                {
                                    candidate = Some(*entity);
                                    break;
                                }
                            }
                            // If all entities are mapped, just use the first one (fallback)
                            let entity_to_map = candidate.unwrap_or(entities[0]);
                            self.scenario
                                .entity_registry_mut()
                                .map_client_entity(entity_key, client_key, entity_to_map);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Context for server-side expectations
pub struct ServerExpectCtx<'b, 'a: 'b> {
    expect_ctx: &'b mut ExpectCtx<'a>,
}

impl<'b, 'a: 'b> ServerExpectCtx<'b, 'a> {
    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&mut self, entity: EntityKey) {
        self.expect_ctx
            .add_expectation(Expectation::ServerHasEntity(entity));
    }

    /// Expect that the server will produce at least one world event of type T
    pub fn event<T: 'static>(&mut self, label: &str) {
        let type_id = TypeId::of::<T>();
        self.expect_ctx
            .add_expectation(Expectation::ServerEvent(type_id, label.to_string()));
    }
}

/// Context for client-side expectations
pub struct ClientExpectCtx<'b, 'a: 'b> {
    expect_ctx: &'b mut ExpectCtx<'a>,
    client_key: ClientKey,
}

impl<'b, 'a: 'b> ClientExpectCtx<'b, 'a> {
    /// Expect that this client will eventually see the logical entity
    pub fn sees(&mut self, entity: EntityKey) {
        self.expect_ctx
            .add_expectation(Expectation::ClientSeesEntity(self.client_key, entity));
    }

    /// Return an expectation view for that logical entity on this client
    pub fn entity(&mut self, entity: EntityKey) -> ClientEntityExpect<'_, 'a> {
        // Ensure mapping exists (implicitly calling sees if needed)
        self.sees(entity);
        // Use the same lifetime as expect_ctx
        ClientEntityExpect {
            expect_ctx: self.expect_ctx,
            client_key: self.client_key,
            entity_key: entity,
        }
    }
}

/// Expectation view for a specific entity on a client
pub struct ClientEntityExpect<'b, 'a: 'b> {
    expect_ctx: &'b mut ExpectCtx<'a>,
    client_key: ClientKey,
    entity_key: EntityKey,
}

impl<'b, 'a: 'b> ClientEntityExpect<'b, 'a> {
    /// Assert that the client's replication configuration for this entity is Delegated
    pub fn replication_is_delegated(self) {
        self.expect_ctx.add_expectation(Expectation::ClientReplicationIsDelegated(
            self.client_key,
            self.entity_key,
        ));
    }

    /// Assert that the client's authority status for this entity equals expected
    pub fn auth_is(self, expected: EntityAuthStatus) {
        self.expect_ctx.add_expectation(Expectation::ClientAuthIs(
            self.client_key,
            self.entity_key,
            expected,
        ));
    }

    /// Assert that the client's position for this entity equals (expected_x, expected_y)
    pub fn position_is(self, expected_x: f32, expected_y: f32) {
        self.expect_ctx.add_expectation(Expectation::ClientPositionIs(
            self.client_key,
            self.entity_key,
            expected_x,
            expected_y,
        ));
    }
}


