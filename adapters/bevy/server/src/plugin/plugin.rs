use std::{ops::DerefMut, sync::Mutex};

use bevy::{
    app::{AppBuilder, CoreStage, Plugin},
    ecs::schedule::SystemStage,
    prelude::*,
};

use naia_server::{ProtocolType, Server, ServerAddrs, ServerConfig, SharedConfig};

use crate::world::entity::Entity;

use super::{stages::ServerStage, systems::should_tick};

struct ServerPluginConfig<P: ProtocolType> {
    server_config: ServerConfig,
    shared_config: SharedConfig<P>,
    server_addrs: ServerAddrs,
}

impl<P: ProtocolType> ServerPluginConfig<P> {
    pub fn new(
        server_config: ServerConfig,
        shared_config: SharedConfig<P>,
        server_addresses: ServerAddrs,
    ) -> Self {
        ServerPluginConfig {
            server_config,
            shared_config,
            server_addrs: server_addresses,
        }
    }
}

pub struct ServerPlugin<P: ProtocolType> {
    config: Mutex<Option<ServerPluginConfig<P>>>,
}

impl<P: ProtocolType> ServerPlugin<P> {
    pub fn new(
        server_config: ServerConfig,
        shared_config: SharedConfig<P>,
        server_addresses: ServerAddrs,
    ) -> Self {
        let config = ServerPluginConfig::new(server_config, shared_config, server_addresses);
        return ServerPlugin {
            config: Mutex::new(Some(config)),
        };
    }
}

impl<P: ProtocolType> Plugin for ServerPlugin<P> {
    fn build(&self, app: &mut AppBuilder) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let mut server = Server::<P, Entity>::new(config.server_config, config.shared_config);
        server.listen(config.server_addrs);

        app
        // RESOURCES //
            .insert_resource(server)

        // STAGES //
            // ServerEvents //
            .add_stage_before(CoreStage::PreUpdate, ServerStage::ServerEvents,
                             SystemStage::single_threaded())
            // Tick //
            .add_stage_after(CoreStage::PostUpdate, ServerStage::Tick,
                             SystemStage::single_threaded()
                                        .with_run_criteria(should_tick::<P>.system()));
    }
}
