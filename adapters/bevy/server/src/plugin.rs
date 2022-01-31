use std::{ops::DerefMut, sync::Mutex};

use bevy::{
    app::{App, CoreStage, Plugin as PluginType},
    ecs::schedule::SystemStage,
    prelude::*,
};
use naia_server::{Protocolize, Server, ServerConfig, SharedConfig};
use naia_bevy_shared::WorldData;

use super::{
    events::{AuthorizationEvent, CommandEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    resource::ServerResource,
    stage::{PrivateStage, Stage},
    systems::{before_receive_events, finish_tick, should_receive, should_tick},
};

struct PluginConfig<P: Protocolize> {
    server_config: ServerConfig,
    shared_config: SharedConfig<P>,
}

impl<P: Protocolize> PluginConfig<P> {
    pub fn new(server_config: ServerConfig, shared_config: SharedConfig<P>) -> Self {
        PluginConfig {
            server_config,
            shared_config,
        }
    }
}

pub struct Plugin<P: Protocolize> {
    config: Mutex<Option<PluginConfig<P>>>,
}

impl<P: Protocolize> Plugin<P> {
    pub fn new(server_config: ServerConfig, shared_config: SharedConfig<P>) -> Self {
        let config = PluginConfig::new(server_config, shared_config);
        return Plugin {
            config: Mutex::new(Some(config)),
        };
    }
}

impl<P: Protocolize> PluginType for Plugin<P> {
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let server = Server::<P, Entity>::new(config.server_config, config.shared_config);

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
                              SystemStage::single_threaded()
                                  .with_run_criteria(should_receive::<P>))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                             Stage::ReceiveEvents,
                             SystemStage::single_threaded()
                                 .with_run_criteria(should_receive::<P>))
            .add_stage_after(CoreStage::PostUpdate,
                              Stage::Tick,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_tick))
            .add_stage_after(Stage::Tick,
                              PrivateStage::AfterTick,
                              SystemStage::parallel()
                                 .with_run_criteria(should_tick))
        // SYSTEMS //
            .add_system_to_stage(PrivateStage::BeforeReceiveEvents,
                                 before_receive_events::<P>.exclusive_system())
            .add_system_to_stage(PrivateStage::AfterTick,
                                 finish_tick);
    }
}
