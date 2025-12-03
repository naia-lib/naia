use crate::harness::ClientMutateCtx;
use crate::harness::server_mutate_ctx::ServerMutateCtx;
use super::scenario::Scenario;
use super::keys::{ClientKey};

/// Context for performing actions in a mutate phase
pub struct MutateCtx<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> MutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    /// Perform server-side actions
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerMutateCtx) -> R) -> R {
        let mut ctx = ServerMutateCtx::new(self.scenario);
        f(&mut ctx)
    }

    /// Perform client-side actions
    pub fn client<R>(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientMutateCtx) -> R) -> R {
        let mut ctx = ClientMutateCtx::new(self.scenario, client);
        f(&mut ctx)
    }
}