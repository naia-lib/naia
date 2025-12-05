use naia_client::{EntityMut, EntityRef};
use naia_demo_world::{WorldRef, WorldMut};

use crate::TestEntity;
use super::scenario::Scenario;
use super::keys::{ClientKey, EntityKey};

/// Lightweight handle for client-side mutations
/// Provides direct pass-through to core Client API with EntityKey resolution
pub struct ClientMutateCtx<'scenario> {
    scenario: &'scenario mut Scenario,
    client_key: ClientKey,
    user_key: naia_server::UserKey,
}

impl<'scenario> ClientMutateCtx<'scenario> {
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
    pub fn spawn<F>(&mut self, f: F) -> EntityKey
    where
        F: for<'a> FnOnce(EntityMut<'a, TestEntity, WorldMut<'a>>),
    {
        // Use a single borrow of state and scoped blocks to manage borrows
        let state = self.scenario.client_state_mut(self.client_key);
        
        // 1. Spawn entity via Client API
        let entity_mut = state.client.spawn_entity(state.world.proxy_mut());
        
        // 2. Get entity ID and LocalEntity before closure consumes entity_mut
        let client_entity = entity_mut.id();
        let local_entity = entity_mut.local_entity()
            .expect("Client-spawned entity should have LocalEntity immediately");
        
        // 3. Call closure with EntityMut (this consumes entity_mut, dropping its borrows)
        f(entity_mut);
        // Now entity_mut is dropped, so we can borrow scenario again
        
        // 4. Allocate EntityKey
        let entity_key = self.scenario.entity_registry_mut().allocate_entity_key();

        // 5. Register spawning client's TestEntity and LocalEntity mapping immediately
        // This allows the server to look up the EntityKey when it receives the spawn event
        println!("[CLIENT_SPAWN] Client {:?} spawned entity: EntityKey={:?}, LocalEntity={:?}", 
            self.client_key, entity_key, local_entity);
        self.scenario.entity_registry_mut()
            .register_client_entity(entity_key, self.client_key, client_entity, local_entity);

        // 7. Return EntityKey - server entity will be registered automatically in tick_once()
        entity_key
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(
        &'_ mut self,
        key: EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        self.scenario.client_entity_ref(self.client_key, self.user_key, key)
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        key: EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        // Delegate to Scenario helper to avoid double-borrow issues
        // The helper uses a single client_state_mut() call with scoped borrows
        self.scenario.client_entity_mut(self.client_key, self.user_key, key)
    }
}

