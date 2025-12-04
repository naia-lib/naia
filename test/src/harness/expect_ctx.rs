use crate::harness::client_expect_ctx::ClientExpectCtx;
use crate::harness::server_expect_ctx::ServerExpectCtx;
use super::scenario::Scenario;
use super::keys::ClientKey;

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

}