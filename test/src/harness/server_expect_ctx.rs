use std::any::TypeId;

use crate::harness::expect_ctx::Expectation;
use crate::harness::ExpectCtx;
use super::keys::EntityKey;

/// Context for server-side expectations
pub struct ServerExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
}

impl<'b, 'a: 'b> ServerExpectCtx<'b, 'a> {
    /// Expect that the server has replicated/created a concrete entity
    pub fn has_entity(&mut self, entity: EntityKey) {
        self.expect_ctx
            .add_expectation(Expectation::ServerHasEntity(entity));
    }

    /// Expect that the server will produce at least one world event of type T
    pub fn event<T: 'static>(&mut self, label: &str) {
        let type_id = TypeId::of::<T>();
        self.expect_ctx
            .add_expectation(Expectation::ServerEvent(type_id, label.to_string()));
    }
}