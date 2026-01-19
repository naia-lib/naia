use super::client_mutate_ctx::ClientMutateCtx;
use super::keys::ClientKey;
use super::scenario::Scenario;
use super::server_mutate_ctx::ServerMutateCtx;

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
    pub fn client<R>(
        &mut self,
        client_key: ClientKey,
        f: impl FnOnce(&mut ClientMutateCtx<'_, '_>) -> R,
    ) -> R {
        let mut ctx = ClientMutateCtx::new(self, client_key);
        f(&mut ctx)
    }

    /// Inject a raw packet from a client to the server
    pub fn inject_client_packet(&mut self, client_key: &ClientKey, data: Vec<u8>) -> bool {
        self.scenario.inject_client_packet(client_key, data)
    }

    /// Push a labeled trace event for deterministic ordering assertions.
    ///
    /// Events are appended in order and can be queried in expect phases
    /// to verify the order of operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// scenario.mutate(|ctx| {
    ///     ctx.trace_push("operation_A");
    ///     ctx.server(|server| { /* ... */ });
    ///     ctx.trace_push("operation_B");
    ///     ctx.server(|server| { /* ... */ });
    /// });
    /// ```
    pub fn trace_push(&mut self, label: impl Into<String>) {
        self.scenario.trace_push(label);
    }
}
