use std::hash::Hash;

use super::{server::WorldServer, user::UserKey};

/// Scoped read-only handle for a user's fine-grained entity scope.
///
/// Obtained from [`Server::user_scope`]. Fine-grained scope is the second
/// layer of visibility control, layered on top of room membership — an entity
/// is only replicated to a user if it is both in a shared room **and** in the
/// user's explicit scope (or if the server uses room-only scoping with no
/// per-entity overrides).
pub struct UserScopeRef<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserScopeRef<'s, E> {
    pub(crate) fn new(server: &'s WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns `true` if the entity is currently in this user's explicit scope.
    pub fn has(&self, world_entity: &E) -> bool {
        self.server.user_scope_has_entity(&self.key, world_entity)
    }
}

/// Scoped mutable handle for a user's fine-grained entity scope.
///
/// Obtained from [`Server::user_scope_mut`]. Use this to include or exclude
/// individual entities from a user's view, independently of room membership.
///
/// # Example
///
/// ```no_run
/// # fn example(server: &mut naia_server::Server<u32>, user_key: &naia_server::UserKey, entity: &u32) {
/// server.user_scope_mut(user_key)
///     .include(entity);
/// # }
/// ```
pub struct UserScopeMut<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s mut WorldServer<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserScopeMut<'s, E> {
    pub(crate) fn new(server: &'s mut WorldServer<E>, key: &UserKey) -> Self {
        Self { server, key: *key }
    }

    /// Returns `true` if the entity is currently in this user's explicit scope.
    pub fn has(&self, world_entity: &E) -> bool {
        self.server.user_scope_has_entity(&self.key, world_entity)
    }

    /// Adds an entity to this user's explicit scope.
    ///
    /// If the entity is also in a room the user belongs to, it will begin
    /// replicating to the user from the next tick.
    pub fn include(&mut self, world_entity: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, world_entity, true);

        self
    }

    /// Removes an entity from this user's explicit scope.
    ///
    /// The entity will be despawned on the user's side unless the entity's
    /// `ScopeExit` is `Persist`.
    pub fn exclude(&mut self, world_entity: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, world_entity, false);

        self
    }

    /// Removes all entities from this user's explicit scope.
    ///
    /// Equivalent to calling `exclude` on every entity currently included.
    pub fn clear(&mut self) -> &mut Self {
        self.server.user_scope_remove_user(&self.key);

        self
    }
}
