use std::{ops::DerefMut, sync::Mutex};

use bevy::{
    app::{App, CoreStage, Plugin as PluginType},
    ecs::schedule::SystemStage,
    prelude::*,
};

use naia_server::{ProtocolType, Server, ServerAddrs, ServerConfig, SharedConfig};

use naia_bevy_shared::WorldData;

use super::{
    events::{AuthorizationEvent, CommandEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    resource::ServerResource,
    stage::{PrivateStage, Stage},
    systems::{before_receive_events, finish_tick, should_tick},
};

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
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let mut server = Server::<P, Entity>::new(config.server_config, config.shared_config);
        server.listen(config.server_addrs);

        app
        // RESOURCES //
            .insert_resource(server)
            .insert_resource(ServerResource::new())
            .insert_resource(WorldData::<P>::new())
        // EVENTS //
            .add_event::<AuthorizationEvent<P>>()
            .add_event::<ConnectionEvent>()
            .add_event::<DisconnectionEvent>()
            .add_event::<MessageEvent<P>>()
            .add_event::<CommandEvent<P>>()
        // STAGES //
            .add_stage_before(CoreStage::PreUpdate,
                              PrivateStage::BeforeReceiveEvents,
                              SystemStage::single_threaded())
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::ReceiveEvents,
                              SystemStage::single_threaded())
            .add_stage_after(CoreStage::PostUpdate,
                              Stage::Tick,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_tick.system()))
            .add_stage_after(Stage::Tick,
                              PrivateStage::AfterTick,
                              SystemStage::parallel()
                                 .with_run_criteria(should_tick.system()))
        // SYSTEMS //
            .add_system_to_stage(PrivateStage::BeforeReceiveEvents,
                                 before_receive_events::<P>.exclusive_system())
            .add_system_to_stage(PrivateStage::AfterTick,
                                 finish_tick.system());
    }
}
