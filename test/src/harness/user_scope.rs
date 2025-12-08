use naia_server::{UserScopeRef as NaiaUserScopeRef, UserScopeMut as NaiaUserScopeMut};

use crate::{TestEntity, harness::{EntityKey, entity_registry::EntityRegistry}};

/// Harness wrapper for UserScopeRef that works with EntityKey instead of TestEntity
pub struct UserScopeRef<'a> {
    scope: NaiaUserScopeRef<'a, TestEntity>,
    registry: &'a EntityRegistry,
}

impl<'a> UserScopeRef<'a> {
    pub(crate) fn new(scope: NaiaUserScopeRef<'a, TestEntity>, registry: &'a EntityRegistry) -> Self {
        Self { scope, registry }
    }

    /// Returns true if the User's scope contains the Entity
    pub fn has(&self, entity_key: &EntityKey) -> bool {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.scope.has(&entity)
        } else {
            false
        }
    }
}

/// Harness wrapper for UserScopeMut that works with EntityKey instead of TestEntity
pub struct UserScopeMut<'a> {
    scope: NaiaUserScopeMut<'a, TestEntity>,
    registry: &'a EntityRegistry,
}

impl<'a> UserScopeMut<'a> {
    pub(crate) fn new(scope: NaiaUserScopeMut<'a, TestEntity>, registry: &'a EntityRegistry) -> Self {
        Self { scope, registry }
    }

    /// Returns true if the User's scope contains the Entity
    pub fn has(&self, entity_key: &EntityKey) -> bool {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.scope.has(&entity)
        } else {
            false
        }
    }

    /// Adds an Entity to the User's scope
    pub fn include(&mut self, entity_key: &EntityKey) -> &mut Self {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.scope.include(&entity);
        }
        self
    }

    /// Removes an Entity from the User's scope
    pub fn exclude(&mut self, entity_key: &EntityKey) -> &mut Self {
        if let Some(entity) = self.registry.server_entity(entity_key) {
            self.scope.exclude(&entity);
        }
        self
    }

    /// Removes all Entities from the User's scope
    pub fn clear(&mut self) -> &mut Self {
        self.scope.clear();
        self
    }
}

