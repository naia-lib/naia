use naia_server::{EntityMut, EntityRef};
use naia_demo_world::{WorldRef, WorldMut};

use crate::{harness::{EntityKey, ClientKey, users::Users, entity_registry::EntityRegistry, user_scope::{UserScopeRef, UserScopeMut}}, TestEntity, TestWorld};

type Server = naia_server::Server<TestEntity>;

/// Lightweight handle for server-side mutations
/// Provides direct pass-through to core Server API with EntityKey resolution
pub struct ServerMutateCtx<'scenario> {
    server: &'scenario mut Server,
    world: &'scenario mut TestWorld,
    registry: &'scenario mut EntityRegistry,
    users: Users<'scenario>,
}

impl<'scenario> ServerMutateCtx<'scenario> {
    pub(crate) fn new(
        server: &'scenario mut Server,
        world: &'scenario mut TestWorld,
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

        // 3. Register server entity
        let entity = entity_mut.id();
        self.registry.register_server_entity(&entity_key, &entity);

        // 4. Call closure with EntityMut
        let result = f(entity_mut);

        // 5. Return (EntityKey, R)
        (entity_key, result)
    }

    /// Get read-only entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityRef<'_, TestEntity, WorldRef<'_>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.server_entity(key)?;

        // 2. Get WorldRef with method lifetime
        let world_ref = self.world.proxy();

        // 3. Call server.entity() and return
        Some(self.server.entity(world_ref, &entity))
    }

    /// Get mutable entity access by EntityKey
    /// Uses method lifetime 'b, not struct lifetime 'scenario
    pub fn entity_mut(
        &'_ mut self,
        key: &EntityKey,
    ) -> Option<EntityMut<'_, TestEntity, WorldMut<'_>>> {
        // 1. Resolve EntityKey to TestEntity
        let entity = self.registry.server_entity(key)?;

        // 2. Get WorldMut with method lifetime
        let world_mut = self.world.proxy_mut();

        // 3. Call server.entity_mut() and return
        Some(self.server.entity_mut(world_mut, &entity))
    }

    /// Returns a HarnessUserScopeRef, which is used to query whether a given user has
    /// entities in scope. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope(&'_ self, client_key: &ClientKey) -> Option<UserScopeRef<'_>> {
        // 1. Get UserKey via users handle
        let user_key = self.users.user_for_client(*client_key)?;

        // 2. Call server.user_scope() to get the underlying scope
        let scope = self.server.user_scope(&user_key);

        // 3. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeRef::new(scope, self.registry))
    }

    /// Returns a HarnessUserScopeMut, which is used to include/exclude Entities for a
    /// given User. Takes ClientKey and converts it to UserKey internally.
    /// The returned scope works with EntityKey instead of TestEntity.
    pub fn user_scope_mut(&'_ mut self, client_key: &ClientKey) -> Option<UserScopeMut<'_>> {
        // 1. Get UserKey via users handle
        let user_key = self.users.user_for_client(*client_key)?;

        // 2. Call server.user_scope_mut() to get the underlying scope
        let scope = self.server.user_scope_mut(&user_key);

        // 3. Wrap it with the harness type that handles EntityKey conversion
        Some(UserScopeMut::new(scope, self.registry))
    }
}

