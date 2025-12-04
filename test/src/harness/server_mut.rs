use naia_server::{EntityMut, EntityRef};
use naia_demo_world::{WorldRef, WorldMut};

use crate::TestEntity;
use super::keys::{ClientKey, EntityKey};
use super::entity_registry::EntityRegistry;
use super::users::Users;

type Server = naia_server::Server<TestEntity>;

/// Lightweight handle for server-side mutations
/// Provides direct pass-through to core Server API with EntityKey resolution
pub struct ServerMut<'scenario> {
    server: &'scenario mut Server,
    world: &'scenario mut crate::TestWorld,
    registry: &'scenario mut EntityRegistry,
    users: Users<'scenario>,
}

impl<'scenario> ServerMut<'scenario> {
    pub(crate) fn new(
        server: &'scenario mut Server,
        world: &'scenario mut crate::TestWorld,
        registry: &'scenario mut EntityRegistry,
        users: Users<'scenario>,
    ) -> Self {
        Self {
            server,
            world,
            registry,
            users,
        }
    }

    /// Spawn a server entity, configure it, and return EntityKey
    pub fn spawn<F, R>(&mut self, f: F) -> (EntityKey, R)
    where
        F: for<'a> FnOnce(EntityMut<'a, TestEntity, WorldMut<'a>>) -> R,
    {
        // 1. Spawn entity via Server API
        let entity_mut = self.server.spawn_entity(self.world.proxy_mut());

        // 2. Allocate EntityKey
        let entity_key = self.registry.allocate_entity_key();

        // 3. Register host entity
        let entity = entity_mut.id();
        self.registry.register_host_entity(entity_key, entity);

        // 4. Call closure with EntityMut
        let result = f(entity_mut);

        // 5. Return (EntityKey, R)
        (entity_key, result)
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity<'b>(
        &'b mut self,
        key: EntityKey,
    ) -> Option<EntityRef<'b, TestEntity, WorldRef<'b>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.host_world(key)?;

        // 2. Get WorldRef with method lifetime
        let world_ref = self.world.proxy();

        // 3. Call server.entity() and return
        Some(self.server.entity(world_ref, &entity))
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut<'b>(
        &'b mut self,
        key: EntityKey,
    ) -> Option<EntityMut<'b, TestEntity, WorldMut<'b>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.host_world(key)?;

        // 2. Get WorldMut with method lifetime
        let world_mut = self.world.proxy_mut();

        // 3. Call server.entity_mut() and return
        Some(self.server.entity_mut(world_mut, &entity))
    }

    /// Helper: include entity in client's scope
    pub fn include_in_scope(&mut self, client_key: ClientKey, entity_key: EntityKey) {
        // 1. Get UserKey via users handle
        let user_key = self.users.user_for_client(client_key)
            .expect("ClientKey not found in users mapping");

        // 2. Resolve entity_key to TestEntity
        let entity = self.registry.host_world(entity_key)
            .expect("EntityKey not registered with host entity");
        
        // 3. Call server.user_scope_mut().include()
        // Note: user_scope_mut doesn't take a world parameter
        self.server.user_scope_mut(&user_key).include(&entity);
    }
}

