use naia_shared::{EntityAuthStatus, WorldRefType};

use crate::{Position, harness::ExpectCtx};
use super::keys::{ClientKey, EntityKey};

/// Context for client-side expectations
pub struct ClientExpectCtx<'b, 'a: 'b> {
    pub(crate) expect_ctx: &'b mut ExpectCtx<'a>,
    pub(crate) client_key: ClientKey,
}

impl<'b, 'a: 'b> ClientExpectCtx<'b, 'a> {
    /// Expect that this client will eventually see the logical entity
    pub fn sees(&mut self, entity: EntityKey) -> bool {
        if self.expect_ctx.scenario.entity_registry().has_client_entity(entity, self.client_key) {
            true
        } else {
            self.expect_ctx.auto_discover_client_entity(self.client_key, entity)
        }
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
    pub fn replication_is_delegated(self) -> bool {
        if let Some(entity) = self.expect_ctx.scenario.entity_registry()
            .get_client_entity(self.entity_key, self.client_key)
        {
            let state = self.expect_ctx.scenario.client_state_mut(self.client_key);
            state
                .client
                .entity_replication_config(&entity)
                .map(|config| config.is_delegated())
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Assert that the client's authority status for this entity equals expected
    pub fn auth_is(self, expected: EntityAuthStatus) -> bool {
        if let Some(entity) = self.expect_ctx.scenario.entity_registry()
            .get_client_entity(self.entity_key, self.client_key)
        {
            let state = self.expect_ctx.scenario.client_state_mut(self.client_key);
            state.client.entity_authority_status(&entity) == Some(expected)
        } else {
            false
        }
    }

    /// Assert that the client's position for this entity equals (expected_x, expected_y)
    pub fn position_is(self, expected_x: f32, expected_y: f32) -> bool {
        if let Some(entity) = self.expect_ctx.scenario.entity_registry()
            .get_client_entity(self.entity_key, self.client_key)
        {
            let state = self.expect_ctx.scenario.client_state_mut(self.client_key);
            if let Some(pos) = state.world.proxy().component::<Position>(&entity) {
                (*pos.x - expected_x).abs() < 0.001 && (*pos.y - expected_y).abs() < 0.001
            } else {
                false
            }
        } else {
            false
        }
    }
}


