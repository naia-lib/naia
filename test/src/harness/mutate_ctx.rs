use super::scenario::Scenario;
use super::keys::{ClientKey};
use super::server_mutate_ctx::ServerMutateCtx;
use super::client_mutate_ctx::ClientMutateCtx;

/// Context for performing actions in a mutate phase
pub struct MutateCtx<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> MutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    pub(crate) fn scenario(&self) -> &Scenario {
        self.scenario
    }

    pub(crate) fn scenario_mut(&mut self) -> &mut Scenario {
        self.scenario
    }

    /// Perform server-side actions
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerMutateCtx<'_, '_>) -> R) -> R {
        let mut ctx = ServerMutateCtx::new(self);
        f(&mut ctx)
    }

    /// Perform client-side actions
    pub fn client<R>(&mut self, client_key: ClientKey, f: impl FnOnce(&mut ClientMutateCtx<'_, '_>) -> R) -> R {
        let mut ctx = ClientMutateCtx::new(self, client_key);
        f(&mut ctx)
    }
}