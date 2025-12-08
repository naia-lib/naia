use std::any::TypeId;

use naia_server::DelegateEntityEvent;

use crate::harness::{ExpectCtx, user_scope::UserScopeRef, EntityKey, ClientKey};

/// Context for server-side expectations
pub struct ServerExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
}

impl<'b, 'a: 'b> ServerExpectCtx<'b, 'a> {
    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&mut self, entity: &EntityKey) -> bool {
        self.expect_ctx.scenario.server_host_entity(entity).is_some()
    }

    /// Expect that the server will produce at least one world event of type T
    pub fn event<T: 'static>(&mut self, _label: &str) -> bool {
        // For now, just check for DelegateEntityEvent
        if TypeId::of::<T>() == TypeId::of::<DelegateEntityEvent>() {
            let mut events = self.expect_ctx.scenario.take_server_events();
            let mut found = false;
            for _ in events.read::<DelegateEntityEvent>() {
                found = true;
                break;
            }
            found
        } else {
            false
        }
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        let scenario = &self.expect_ctx.scenario;
        
        // 1. Get UserKey via helper method
        let user_key = scenario.user_key_for_client(client_key)?;

        // 2. Get server and registry immutably
        let (server, registry) = scenario.server_and_registry()?;

        // 3. Call server.user_scope() to get the underlying scope
        let scope = server.user_scope(&user_key);

        // 4. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeRef::new(scope, registry))
    }
}