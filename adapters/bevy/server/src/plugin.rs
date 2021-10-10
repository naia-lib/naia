use std::{ops::DerefMut, sync::Mutex};

use bevy::app::{AppBuilder, Plugin as PluginType};

use naia_server::{ProtocolType, Server, ServerAddrs, ServerConfig, SharedConfig};

use naia_bevy_shared::Entity;

use super::ticker::Ticker;

struct PluginConfig<P: ProtocolType> {
    server_config: ServerConfig,
    shared_config: SharedConfig<P>,
    server_addrs: ServerAddrs,
}

impl<P: ProtocolType> PluginConfig<P> {
    pub fn new(
        server_config: ServerConfig,
        shared_config: SharedConfig<P>,
        server_addresses: ServerAddrs,
    ) -> Self {
        PluginConfig {
            server_config,
            shared_config,
            server_addrs: server_addresses,
        }
    }
}

pub struct Plugin<P: ProtocolType> {
    config: Mutex<Option<PluginConfig<P>>>,
}

impl<P: ProtocolType> Plugin<P> {
    pub fn new(
        server_config: ServerConfig,
        shared_config: SharedConfig<P>,
        server_addresses: ServerAddrs,
    ) -> Self {
        let config = PluginConfig::new(server_config, shared_config, server_addresses);
        return Plugin {
            config: Mutex::new(Some(config)),
        };
    }
}

impl<P: ProtocolType> PluginType for Plugin<P> {
    fn build(&self, app: &mut AppBuilder) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let mut server = Server::<P, Entity>::new(config.server_config, config.shared_config);
        server.listen(config.server_addrs);

        app
        // RESOURCES //
            .insert_resource(server)
            .insert_resource(Ticker::new());
    }
}
