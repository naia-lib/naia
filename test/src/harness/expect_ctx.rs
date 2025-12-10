use std::collections::HashMap;

use naia_server::Events as ServerEvents;
use naia_client::WorldEvents as ClientEvents;

use crate::{TestEntity, harness::{keys::ClientKey, scenario::Scenario, server_expect_ctx::ServerExpectCtx, client_expect_ctx::ClientExpectCtx}};

/// Context for evaluating expectations in an expect phase
/// 
/// This is an immutable, per-tick read-only view that exposes:
/// - read-only server/client/world access
/// - the server/client events for that tick
pub struct ExpectCtx<'a> {
    scenario: &'a Scenario,
    server_events: ServerEvents<TestEntity>,
    client_events_map: HashMap<ClientKey, ClientEvents<TestEntity>>,
}

impl<'a> ExpectCtx<'a> {
    pub(crate) fn new(
        scenario: &'a Scenario,
        server_events: ServerEvents<TestEntity>,
        client_events_map: HashMap<ClientKey, ClientEvents<TestEntity>>,
    ) -> Self {
        Self {
            scenario,
            server_events,
            client_events_map,
        }
    }

    /// Access server-side expectations with per-tick events
    pub fn server<R>(&mut self, f: impl FnOnce(&mut ServerExpectCtx<'_>) -> R) -> R {
        let mut server_expect = ServerExpectCtx::new(
            self.scenario,
            &mut self.server_events,
        );
        f(&mut server_expect)
    }

    /// Access client-side expectations with per-tick events
    pub fn client<R>(&mut self, client_key: ClientKey, f: impl FnOnce(&mut ClientExpectCtx<'_>) -> R) -> R {
        let client_events = self.client_events_map
            .entry(client_key)
            .or_insert_with(ClientEvents::default);
        
        let mut client_expect = ClientExpectCtx::new(
            self.scenario,
            client_key,
            client_events,
        );
        f(&mut client_expect)
    }
}