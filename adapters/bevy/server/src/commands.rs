use bevy_ecs::system::EntityCommands;

use naia_bevy_shared::HostOwned;

use crate::Server;

// Bevy Commands Extension
pub trait CommandsExt<'w, 's, 'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn disable_replication(&'a mut self, server: &mut Server)
        -> &'a mut EntityCommands<'w, 's, 'a>;
}

impl<'w, 's, 'a> CommandsExt<'w, 's, 'a> for EntityCommands<'w, 's, 'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.enable_replication(&self.id());
        self.insert(HostOwned);
        return self;
    }

    fn disable_replication(
        &'a mut self,
        server: &mut Server,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        server.disable_replication(&self.id());
        self.remove::<HostOwned>();
        return self;
    }
}
