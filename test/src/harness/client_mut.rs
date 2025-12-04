use naia_client::{EntityMut, EntityRef};
use naia_demo_world::{WorldRef, WorldMut};

use crate::TestEntity;
use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Lightweight handle for client-side mutations
/// Provides direct pass-through to core Client API with EntityKey resolution
pub struct ClientMut<'scenario> {
    scenario: &'scenario mut Scenario,
    client_key: ClientKey,
    user_key: naia_server::UserKey,
}

impl<'scenario> ClientMut<'scenario> {
    pub(crate) fn new(
        scenario: &'scenario mut Scenario,
        client_key: ClientKey,
        user_key: naia_server::UserKey,
    ) -> Self {
        // ClientMut holds &mut Scenario directly and borrows fields internally when needed
        Self {
            scenario,
            client_key,
            user_key,
        }
    }

    /// Spawn a client entity, configure it, wait for server registration, return EntityKey
    /// Synchronous: waits for server to have entity before returning
    pub fn spawn<F, R>(&mut self, f: F) -> (EntityKey, R)
    where
        F: for<'a> FnOnce(EntityMut<'a, TestEntity, WorldMut<'a>>) -> R,
    {
        // Use a single borrow of state and scoped blocks to manage borrows
        let state = self.scenario.client_state_mut(self.client_key);
        
        // 1. Spawn entity via Client API
        let entity_mut = state.client.spawn_entity(state.world.proxy_mut());
        
        // 2. Get LocalEntity from the spawned entity (EntityMut has local_entity() method)
        let local_entity = entity_mut.local_entity()
            .expect("Client-spawned entity should have LocalEntity immediately");
        
        // 3. Call closure with EntityMut (this consumes entity_mut, dropping its borrows)
        let result = f(entity_mut);
        // Now entity_mut is dropped, so we can borrow scenario again
        
        // 4. Allocate EntityKey
        let entity_key = self.scenario.entity_registry_mut().allocate_entity_key();

        // 5. Register spawning client mapping
        self.scenario.entity_registry_mut()
            .register_spawning_client(entity_key, self.client_key, local_entity);

        // 6. Call scenario.spawn_and_track_client_entity() to wait for server
        self.scenario.spawn_and_track_client_entity(entity_key, self.client_key, local_entity);

        // 7. Return (EntityKey, R)
        (entity_key, result)
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity<'b>(
        &'b mut self,
        key: EntityKey,
    ) -> Option<EntityRef<'b, TestEntity, WorldRef<'b>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        self.scenario.client_entity_ref(self.client_key, self.user_key, key)
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut<'b>(
        &'b mut self,
        key: EntityKey,
    ) -> Option<EntityMut<'b, TestEntity, WorldMut<'b>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        // The helper uses a single client_state_mut() call with scoped borrows
        self.scenario.client_entity_mut(self.client_key, self.user_key, key)
    }
}

