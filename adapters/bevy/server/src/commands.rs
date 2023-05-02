use bevy_ecs::system::EntityCommands;

use naia_bevy_shared::HostOwned;
use naia_server::ReplicationConfig;

use crate::Server;

// Bevy Commands Extension
pub trait CommandsExt<'w, 's, 'a> {
    fn enable_replication(&'a mut self, server: &mut Server) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn disable_replication(&'a mut self, server: &mut Server)
        -> &'a mut EntityCommands<'w, 's, 'a>;
    fn configure_replication(
        &'a mut self,
        server: &mut Server,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a>;
    fn replication_config(&'a self, server: &Server) -> ReplicationConfig;
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

    fn configure_replication(
        &'a mut self,
        server: &mut Server,
        config: ReplicationConfig,
    ) -> &'a mut EntityCommands<'w, 's, 'a> {
        match &config {
            ReplicationConfig::Disabled => {
                self.remove::<HostOwned>();
            }
            _ => {
                self.insert(HostOwned);
            }
        }
        server.configure_replication(&self.id(), config);
        return self;
    }

    fn replication_config(&'a self, server: &Server) -> ReplicationConfig {
        server.replication_config(&self.id())
    }
}
