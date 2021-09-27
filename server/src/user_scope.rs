use naia_shared::ProtocolType;

use crate::UserKey;

use super::{server::Server, world_type::WorldType};

pub struct UserScopeMut<'s, P: ProtocolType, W: WorldType<P>> {
    server: &'s mut Server<P, W>,
    key: UserKey,
}

impl<'s, P: ProtocolType, W: WorldType<P>> UserScopeMut<'s, P, W> {
    pub fn new(server: &'s mut Server<P, W>, key: &UserKey) -> Self {
        UserScopeMut { server, key: *key }
    }

    pub fn include(&mut self, entity_key: &W::EntityKey) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, true);

        self
    }

    pub fn exclude(&mut self, entity_key: &W::EntityKey) -> &mut Self {
        self.server
            .user_scope_set_entity(&self.key, entity_key, false);

        self
    }
}
