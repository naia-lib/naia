use naia_shared::ProtocolType;

use crate::UserKey;

use super::{keys::KeyType, server::Server};

pub struct UserScopeMut<'s, P: ProtocolType, K: KeyType> {
    server: &'s mut Server<P, K>,
    key: UserKey,
}

impl<'s, P: ProtocolType, K: KeyType> UserScopeMut<'s, P, K> {
    pub fn new(server: &'s mut Server<P, K>, key: &UserKey) -> Self {
        UserScopeMut { server, key: *key }
    }

    pub fn include(&mut self, entity_key: &K) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, true);

        self
    }

    pub fn exclude(&mut self, entity_key: &K) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, false);

        self
    }
}
