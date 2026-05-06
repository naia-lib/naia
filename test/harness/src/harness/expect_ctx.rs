use std::collections::HashMap;

use crate::harness::client_events::ClientEvents;
use crate::harness::server_events::ServerEvents;
use crate::harness::{
    client_expect_ctx::ClientExpectCtx, keys::ClientKey, scenario::Scenario,
    server_expect_ctx::ServerExpectCtx,
};

/// Context for evaluating expectations in an expect phase
///
/// This is an immutable, per-tick read-only view that exposes:
/// - read-only server/client/world access
/// - the server/client events for that tick
/// - pre-translated events (AuthEvent/ConnectEvent as ClientKey)
pub struct ExpectCtx<'a> {
    scenario: &'a Scenario,
    server_events: ServerEvents,
    client_events_map: HashMap<ClientKey, ClientEvents>,
}

impl<'a> ExpectCtx<'a> {
    pub(crate) fn new(
        scenario: &'a Scenario,
        server_events: ServerEvents,
        client_events_map: HashMap<ClientKey, ClientEvents>,
    ) -> Self {
        Self {
            scenario,
            server_events,
            client_events_map,
        }
    }

    /// Access server-side expectations with per-tick events
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerExpectCtx<'_>) -> R) -> R {
        let mut server_expect = ServerExpectCtx::new(self.scenario, &mut self.server_events);
        f(&mut server_expect)
    }

    /// Access client-side expectations with per-tick events
    pub fn client<R>(
        &mut self,
        client_key: ClientKey,
        f: impl FnOnce(&mut ClientExpectCtx<'_>) -> R,
    ) -> R {
        let client_events = self
            .client_events_map
            .entry(client_key)
            .or_default();

        let mut client_expect = ClientExpectCtx::new(self.scenario, client_key, client_events);
        f(&mut client_expect)
    }

    /// Get access to scenario for read-only queries.
    ///
    /// This provides access to scenario-level state and history,
    /// such as event ordering assertions and client lookups.
    pub fn scenario(&self) -> &Scenario {
        self.scenario
    }

    /// Get the current iteration count (how many times scenario.tick() has been called)
    /// This is NOT a game tick - it's just the test harness's internal iteration counter.
    pub fn global_tick(&self) -> usize {
        self.scenario.global_tick()
    }
}
