use std::hash::Hash;

use super::{server::InternalWorldServer, user::UserKey};
use crate::PipelinedWorldServer;

/// Which engine shape a [`UserScopeRef`]/[`UserScopeMut`] acts on (task #9).
/// Scope state is **send-resident** (`entity_scope_map`): the Pipelined arm
/// reads via a `&self` slot-lock (`user_scope_has_entity_ref`) and writes via
/// D7 scope-ledger staging — no panic.
enum UserScopeRefTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s InternalWorldServer<E>),
    Pipelined(&'s PipelinedWorldServer<E>),
}

enum UserScopeMutTarget<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    Resident(&'s mut InternalWorldServer<E>),
    Pipelined(&'s mut PipelinedWorldServer<E>),
}

/// Scoped read-only handle for a user's fine-grained entity scope.
///
/// Obtained from [`Server::user_scope`]. Fine-grained scope is the second
/// layer of visibility control, layered on top of room membership — an entity
/// is only replicated to a user if it is both in a shared room **and** in the
/// user's explicit scope (or if the server uses room-only scoping with no
/// per-entity overrides).
pub struct UserScopeRef<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: UserScopeRefTarget<'s, E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> UserScopeRef<'s, E> {
    pub(crate) fn new(server: &'s InternalWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserScopeRefTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s PipelinedWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserScopeRefTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns `true` if the entity is currently in this user's explicit scope.
    pub fn has(&self, world_entity: &E) -> bool {
        match &self.server {
            UserScopeRefTarget::Resident(ws) => ws.user_scope_has_entity(&self.key, world_entity),
            UserScopeRefTarget::Pipelined(ps) => {
                ps.user_scope_has_entity_ref(&self.key, world_entity)
            }
        }
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
pub struct UserScopeMut<'s, E: Copy + Eq + Hash + Send + Sync + 'static> {
    server: UserScopeMutTarget<'s, E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync + 'static> UserScopeMut<'s, E> {
    pub(crate) fn new(server: &'s mut InternalWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserScopeMutTarget::Resident(server),
            key: *key,
        }
    }

    pub(crate) fn with_pipeline(server: &'s mut PipelinedWorldServer<E>, key: &UserKey) -> Self {
        Self {
            server: UserScopeMutTarget::Pipelined(server),
            key: *key,
        }
    }

    /// Returns `true` if the entity is currently in this user's explicit scope.
    pub fn has(&self, world_entity: &E) -> bool {
        match &self.server {
            UserScopeMutTarget::Resident(ws) => ws.user_scope_has_entity(&self.key, world_entity),
            UserScopeMutTarget::Pipelined(ps) => {
                ps.user_scope_has_entity_ref(&self.key, world_entity)
            }
        }
    }

    /// Adds an entity to this user's explicit scope.
    ///
    /// If the entity is also in a room the user belongs to, it will begin
    /// replicating to the user from the next tick.
    pub fn include(&mut self, world_entity: &E) -> &mut Self {
        match &mut self.server {
            UserScopeMutTarget::Resident(ws) => {
                ws.user_scope_set_entity(&self.key, world_entity, true)
            }
            UserScopeMutTarget::Pipelined(ps) => {
                ps.user_scope_set_entity(&self.key, world_entity, true)
            }
        }
        self
    }

    /// Removes an entity from this user's explicit scope.
    ///
    /// The entity will be despawned on the user's side unless the entity's
    /// `ScopeExit` is `Persist`.
    pub fn exclude(&mut self, world_entity: &E) -> &mut Self {
        match &mut self.server {
            UserScopeMutTarget::Resident(ws) => {
                ws.user_scope_set_entity(&self.key, world_entity, false)
            }
            UserScopeMutTarget::Pipelined(ps) => {
                ps.user_scope_set_entity(&self.key, world_entity, false)
            }
        }
        self
    }

    /// Removes all entities from this user's explicit scope.
    ///
    /// Equivalent to calling `exclude` on every entity currently included.
    pub fn clear(&mut self) -> &mut Self {
        match &mut self.server {
            UserScopeMutTarget::Resident(ws) => ws.user_scope_remove_user(&self.key),
            UserScopeMutTarget::Pipelined(ps) => ps.user_scope_remove_user(&self.key),
        }
        self
    }
}
