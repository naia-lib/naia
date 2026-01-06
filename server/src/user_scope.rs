use std::hash::Hash;

use super::{server::WorldServer, user::UserKey};

pub struct UserScopeRef<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserScopeRef<'s, E> {
    pub(crate) fn new(server: &'s WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns true if the User's scope contains the Entity
    pub fn has(&self, world_entity: &E) -> bool {
        self.server.user_scope_has_entity(&self.key, world_entity)
    }
}

pub struct UserScopeMut<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s mut WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserScopeMut<'s, E> {
    pub(crate) fn new(server: &'s mut WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns true if the User's scope contains the Entity
    pub fn has(&self, world_entity: &E) -> bool {
        self.server.user_scope_has_entity(&self.key, world_entity)
    }

    /// Adds an Entity to the User's scope
    pub fn include(&mut self, world_entity: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, world_entity, true);

        self
    }

    /// Removes an Entity from the User's scope
    pub fn exclude(&mut self, world_entity: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, world_entity, false);

        self
    }

    /// Removes all Entities from the User's scope
    pub fn clear(&mut self) -> &mut Self {
        self.server.user_scope_remove_user(&self.key);

        self
    }
}
