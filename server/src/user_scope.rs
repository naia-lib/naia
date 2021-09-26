use naia_shared::{EntityKey, ProtocolType};

use crate::UserKey;

use super::server::Server;

pub struct UserScopeMut<'s, P: ProtocolType> {
    server: &'s mut Server<P>,
    key: UserKey,
}

impl<'s, P: ProtocolType> UserScopeMut<'s, P> {
    pub fn new(server: &'s mut Server<P>, key: &UserKey) -> Self {
        UserScopeMut { server, key: *key }
    }

    pub fn include(&mut self, entity_key: &EntityKey) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, true);

        self
    }

    pub fn exclude(&mut self, entity_key: &EntityKey) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, false);

        self
    }
}
