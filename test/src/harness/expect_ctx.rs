use std::collections::HashMap;

use naia_server::Events as ServerEvents;
use naia_client::WorldEvents as ClientEvents;

use crate::{TestEntity, harness::{keys::ClientKey, scenario::Scenario, server_expect_ctx::ServerExpectCtx, client_expect_ctx::ClientExpectCtx}};

/// Context for evaluating expectations in an expect phase
pub struct ExpectCtx<'a> {
    scenario: &'a mut Scenario,
    max_ticks: usize,
    server_events: Option<ServerEvents<TestEntity>>,
    client_events_map: HashMap<ClientKey, ClientEvents<TestEntity>>
}

impl<'a> ExpectCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario, max_ticks: usize) -> Self {
        Self {
            scenario,
            max_ticks,
            server_events: None,
            client_events_map: HashMap::new()
        }
    }

    /// Override the default maximum tick budget
    pub fn ticks(&mut self, max_ticks: usize) {
        self.max_ticks = max_ticks;
    }

    /// Register server-side expectations
    pub fn server(&self, f: impl Fn(&ServerExpectCtx<'_, 'a>) -> bool) -> bool {
        let ctx = ServerExpectCtx::new(self);
        f(&ctx)
    }

    /// Register client-side expectations
    pub fn client(&self, client: ClientKey, f: impl Fn(&ClientExpectCtx<'_, 'a>) -> bool) -> bool {
        let ctx = ClientExpectCtx::new(self, client);
        f(&ctx)
    }

    pub(crate) fn run<F>(&mut self, mut f: F)
    where
        F: FnMut(&Self) -> bool,
    {
        for tick in 0..self.max_ticks {
            self.scenario.tick_once();

            // Collect server events after each tick
            self.server_events = Some(self.scenario.take_server_events());
            self.client_events_map = self.scenario.take_client_events();

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

    /// Get reference to the scenario
    pub(crate) fn scenario(&self) -> &Scenario {
        self.scenario
    }
}