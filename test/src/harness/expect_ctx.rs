use crate::harness::{keys::ClientKey, scenario::Scenario, server_expect_ctx::ServerExpectCtx, client_expect_ctx::ClientExpectCtx};

/// Context for evaluating expectations in an expect phase
pub struct ExpectCtx<'a> {
    scenario: &'a mut Scenario,
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
        let mut ctx = ServerExpectCtx::new(self);
        f(&mut ctx)
    }

    /// Register client-side expectations
    pub fn client(&mut self, client: ClientKey, mut f: impl FnMut(&mut ClientExpectCtx<'_, 'a>) -> bool) -> bool {
        let mut ctx = ClientExpectCtx::new(self, client);
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

    /// Get mutable reference to the scenario
    pub(crate) fn scenario_mut(&mut self) -> &mut Scenario {
        self.scenario
    }

    /// Get reference to the scenario
    pub(crate) fn scenario(&self) -> &Scenario {
        self.scenario
    }

}