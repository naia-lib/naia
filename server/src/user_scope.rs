use std::hash::Hash;

use naia_shared::ProtocolType;

use crate::UserKey;

use super::server::Server;

pub struct UserScopeMut<'s, P: ProtocolType, E: Copy + Eq + Hash> {
    server: &'s mut Server<P, E>,
    key: UserKey,
}

impl<'s, P: ProtocolType, E: Copy + Eq + Hash> UserScopeMut<'s, P, E> {
    pub fn new(server: &'s mut Server<P, E>, key: &UserKey) -> Self {
        UserScopeMut { server, key: *key }
    }

    pub fn include(&mut self, entity_key: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, true);

        self
    }

    pub fn exclude(&mut self, entity_key: &E) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, false);

        self
    }
}
