use std::hash::Hash;

use super::{server::Server, user::UserKey};

pub struct UserScopeMut<'s, E: Copy + Eq + Hash + Send + Sync> {
    server: &'s mut Server<E>,
    key: UserKey,
}

impl<'s, E: Copy + Eq + Hash + Send + Sync> UserScopeMut<'s, E> {
    pub fn new(server: &'s mut Server<E>, key: &UserKey) -> Self {
        UserScopeMut { server, key: *key }
    }

    /// Adds an Entity to the User's scope
    pub fn include(&mut self, entity: &E) -> &mut Self {
        self.server.user_scope_set_entity(&self.key, entity, true);

        self
    }

    /// Removes an Entity from the User's scope
    pub fn exclude(&mut self, entity: &E) -> &mut Self {
        self.server.user_scope_set_entity(&self.key, entity, false);

        self
    }

    /// Removes all Entities from the User's scope
    pub fn clear(&mut self) -> &mut Self {
        self.server.user_scope_remove_user(&self.key);

        self
    }
}
