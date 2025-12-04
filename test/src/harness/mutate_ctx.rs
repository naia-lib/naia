use crate::harness::ClientMutateCtx;
use crate::harness::server_mutate_ctx::ServerMutateCtx;
use super::scenario::Scenario;
use super::keys::{ClientKey};
use super::server_mut::ServerMut;
use super::client_mut::ClientMut;

/// Context for performing actions in a mutate phase
pub struct MutateCtx<'a> {
    scenario: &'a mut Scenario,
}

impl<'a> MutateCtx<'a> {
    pub(crate) fn new(scenario: &'a mut Scenario) -> Self {
        Self { scenario }
    }

    /// Get server-side mutation handle
    pub fn server_mut(&mut self) -> ServerMut<'_> {
        let (server, world, registry, users) = self.scenario.split_for_server_mut();
        ServerMut::new(server, world, registry, users)
    }

    /// Get client-side mutation handle
    pub fn client(&mut self, client_key: ClientKey) -> ClientMut<'_> {
        // Get user_key without mutably borrowing scenario
        let user_key = self.scenario.user_key(client_key);
        // ClientMut holds &mut Scenario directly and borrows fields internally
        ClientMut::new(self.scenario, client_key, user_key)
    }

    /// Perform server-side actions (old API - kept for backward compatibility)
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerMutateCtx) -> R) -> R {
        let mut ctx = ServerMutateCtx::new(self.scenario);
        f(&mut ctx)
    }

    /// Perform client-side actions (old API - kept for backward compatibility)
    /// Note: This conflicts with the new `client()` method, so it's renamed to `client_with_ctx`
    pub fn client_with_ctx<R>(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientMutateCtx) -> R) -> R {
        let mut ctx = ClientMutateCtx::new(self.scenario, client);
        f(&mut ctx)
    }
}