use std::hash::Hash;

use naia_shared::{ChannelIndex, Protocolize};

use super::{server::Server, user::UserKey};

pub struct UserScopeMut<'s, P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex> {
    server: &'s mut Server<P, E, C>,
    key: UserKey,
}

impl<'s, P: Protocolize, E: Copy + Eq + Hash + Send + Sync, C: ChannelIndex>
    UserScopeMut<'s, P, E, C>
{
    pub fn new(server: &'s mut Server<P, E, C>, key: &UserKey) -> Self {
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
