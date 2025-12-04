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

    /// Perform server-side actions
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerMut<'_>) -> R) -> R {
        let (server, world, registry, users) = self.scenario.split_for_server_mut();
        let mut ctx = ServerMut::new(server, world, registry, users);
        f(&mut ctx)
    }

    /// Perform client-side actions
    pub fn client<R>(&mut self, client_key: ClientKey, f: impl FnOnce(&mut ClientMut<'_>) -> R) -> R {
        // Get user_key without mutably borrowing scenario
        let user_key = self.scenario.user_key(client_key);
        // ClientMut holds &mut Scenario directly and borrows fields internally
        let mut ctx = ClientMut::new(self.scenario, client_key, user_key);
        f(&mut ctx)
    }
}