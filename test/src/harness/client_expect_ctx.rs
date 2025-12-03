
use naia_shared::EntityAuthStatus;

use crate::harness::expect_ctx::Expectation;
use crate::harness::ExpectCtx;
use super::keys::{ClientKey, EntityKey};

/// Context for client-side expectations
pub struct ClientExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
    pub(crate) client_key: ClientKey,
}

impl<'b, 'a: 'b> ClientExpectCtx<'b, 'a> {
    /// Expect that this client will eventually see the logical entity
    pub fn sees(&mut self, entity: EntityKey) {
        self.expect_ctx
            .add_expectation(Expectation::ClientSeesEntity(self.client_key, entity));
    }

    /// Return an expectation view for that logical entity on this client
    pub fn entity(&mut self, entity: EntityKey) -> ClientEntityExpect<'_, 'a> {
        // Ensure mapping exists (implicitly calling sees if needed)
        self.sees(entity);
        // Use the same lifetime as expect_ctx
        ClientEntityExpect {
            expect_ctx: self.expect_ctx,
            client_key: self.client_key,
            entity_key: entity,
        }
    }
}

/// Expectation view for a specific entity on a client
pub struct ClientEntityExpect<'b, 'a: 'b> {
    expect_ctx: &'b mut ExpectCtx<'a>,
    client_key: ClientKey,
    entity_key: EntityKey,
}

impl<'b, 'a: 'b> ClientEntityExpect<'b, 'a> {
    /// Assert that the client's replication configuration for this entity is Delegated
    pub fn replication_is_delegated(self) {
        self.expect_ctx.add_expectation(Expectation::ClientReplicationIsDelegated(
            self.client_key,
            self.entity_key,
        ));
    }

    /// Assert that the client's authority status for this entity equals expected
    pub fn auth_is(self, expected: EntityAuthStatus) {
        self.expect_ctx.add_expectation(Expectation::ClientAuthIs(
            self.client_key,
            self.entity_key,
            expected,
        ));
    }

    /// Assert that the client's position for this entity equals (expected_x, expected_y)
    pub fn position_is(self, expected_x: f32, expected_y: f32) {
        self.expect_ctx.add_expectation(Expectation::ClientPositionIs(
            self.client_key,
            self.entity_key,
            expected_x,
            expected_y,
        ));
    }
}


