use naia_shared::{EntityAuthStatus, WorldRefType};
use naia_server::DelegateEntityEvent;

use crate::Position;
use crate::harness::client_expect_ctx::ClientExpectCtx;
use crate::harness::server_expect_ctx::ServerExpectCtx;
use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

// Import WorldRefType trait to use entities() method
use naia_shared::WorldRefType as _;

/// Context for evaluating expectations in an expect phase
pub struct ExpectCtx<'a> {
    pub(crate) scenario: &'a mut Scenario,
    max_ticks: usize,
}

impl<'a> ExpectCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, max_ticks: usize) -> Self {
        Self {
            scenario,
            max_ticks,
        }
    }

    /// Override the default maximum tick budget
    pub fn ticks(&mut self, max_ticks: usize) {
        self.max_ticks = max_ticks;
    }

    /// Register server-side expectations
    pub fn server(&mut self, mut f: impl FnMut(&mut ServerExpectCtx<'_, 'a>) -> bool) -> bool {
        let mut ctx = ServerExpectCtx { expect_ctx: self };
        f(&mut ctx)
    }

    /// Register client-side expectations
    pub fn client(&mut self, client: ClientKey, mut f: impl FnMut(&mut ClientExpectCtx<'_, 'a>) -> bool) -> bool {
        let mut ctx = ClientExpectCtx {
            expect_ctx: self,
            client_key: client,
        };
        f(&mut ctx)
    }

    pub(crate) fn run<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut Self) -> bool,
    {
        for tick in 0..self.max_ticks {
            self.scenario.tick_once();

            // Evaluate the closure - if it returns true, all expectations passed
            if f(self) {
                return;
            }

            if tick == self.max_ticks - 1 {
                panic!(
                    "Expect phase timed out after {} ticks",
                    self.max_ticks
                );
            }
        }
    }

    /// Auto-discover server entity if not mapped yet
    pub(crate) fn auto_discover_server_entity(&mut self, entity_key: EntityKey) -> bool {
        let entities = self.scenario.server_world_mut().proxy().entities();
        if !entities.is_empty() && !self.scenario.entity_registry().has_server_entity(entity_key) {
            // Use first entity that isn't already mapped to another key
            let mut candidate = None;
            for entity in &entities {
                if !self.scenario.entity_registry().is_server_entity_mapped(*entity) {
                    candidate = Some(*entity);
                    break;
                }
            }
            let entity_to_map = candidate.unwrap_or(entities[0]);
            self.scenario
                .entity_registry_mut()
                .map_server_entity(entity_key, entity_to_map);
            true
        } else {
            false
        }
    }

    /// Auto-discover client entity if not mapped yet
    pub(crate) fn auto_discover_client_entity(&mut self, client_key: ClientKey, entity_key: EntityKey) -> bool {
        let state = self.scenario.client_state_mut(client_key);
        let entities = state.world.proxy().entities();
        
        if !entities.is_empty()
            && !self.scenario.entity_registry().has_client_entity(entity_key, client_key)
        {
            let mut candidate = None;
            for entity in &entities {
                if !self.scenario.entity_registry()
                    .is_client_entity_mapped_to_different_key(*entity, client_key, entity_key)
                {
                    candidate = Some(*entity);
                    break;
                }
            }
            let entity_to_map = candidate.unwrap_or(entities[0]);
            self.scenario
                .entity_registry_mut()
                .map_client_entity(entity_key, client_key, entity_to_map);
            true
        } else {
            false
        }
    }
}