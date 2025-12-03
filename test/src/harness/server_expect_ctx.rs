use std::any::TypeId;

use naia_server::DelegateEntityEvent;

use crate::harness::ExpectCtx;
use super::keys::EntityKey;

/// Context for server-side expectations
pub struct ServerExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
}

impl<'b, 'a: 'b> ServerExpectCtx<'b, 'a> {
    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&mut self, entity: EntityKey) -> bool {
        if self.expect_ctx.scenario.entity_registry().has_server_entity(entity) {
            true
        } else {
            self.expect_ctx.auto_discover_server_entity(entity)
        }
    }

    /// Expect that the server will produce at least one world event of type T
    pub fn event<T: 'static>(&mut self, _label: &str) -> bool {
        // For now, just check for DelegateEntityEvent
        if std::any::TypeId::of::<T>() == TypeId::of::<DelegateEntityEvent>() {
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
}