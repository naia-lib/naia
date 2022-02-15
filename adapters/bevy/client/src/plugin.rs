use std::{ops::DerefMut, sync::Mutex};

use bevy::{
    app::{App, CoreStage, Plugin as PluginType},
    ecs::schedule::SystemStage,
    prelude::*,
};
use naia_bevy_shared::WorldData;
use naia_client::{
    shared::{Protocolize, SharedConfig},
    Client, ClientConfig,
};

use crate::systems::should_receive;

use super::{
    events::{
        DespawnEntityEvent, InsertComponentEvent, MessageEvent, RemoveComponentEvent,
        SpawnEntityEvent, UpdateComponentEvent,
    },
    resource::ClientResource,
    stage::{PrivateStage, Stage},
    systems::{
        before_receive_events, finish_connect, finish_disconnect, finish_tick, should_connect,
        should_disconnect, should_tick,
    },
};

struct PluginConfig<P: Protocolize> {
    client_config: ClientConfig,
    shared_config: SharedConfig<P>,
}

impl<P: Protocolize> PluginConfig<P> {
    pub fn new(client_config: ClientConfig, shared_config: SharedConfig<P>) -> Self {
        PluginConfig {
            client_config,
            shared_config,
        }
    }
}

pub struct Plugin<P: Protocolize> {
    config: Mutex<Option<PluginConfig<P>>>,
}

impl<P: Protocolize> Plugin<P> {
    pub fn new(client_config: ClientConfig, shared_config: SharedConfig<P>) -> Self {
        let config = PluginConfig::new(client_config, shared_config);
        return Plugin {
            config: Mutex::new(Some(config)),
        };
    }
}

impl<P: Protocolize> PluginType for Plugin<P> {
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let client = Client::<P, Entity>::new(config.client_config, config.shared_config);

        app
        // RESOURCES //
            .insert_resource(client)
            .insert_resource(ClientResource::new())
            .insert_resource(WorldData::<P>::new())
        // EVENTS //
            .add_event::<SpawnEntityEvent<P>>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<InsertComponentEvent<P>>()
            .add_event::<UpdateComponentEvent<P>>()
            .add_event::<RemoveComponentEvent<P>>()
            .add_event::<MessageEvent<P>>()
        // STAGES //
            // events //
            .add_stage_before(CoreStage::PreUpdate,
                              PrivateStage::BeforeReceiveEvents,
                              SystemStage::single_threaded()
                                  .with_run_criteria(should_receive::<P>))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                             Stage::ReceiveEvents,
                             SystemStage::single_threaded()
                                 .with_run_criteria(should_receive::<P>))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::Connection,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_connect))
            .add_stage_after(Stage::Connection,
                              PrivateStage::AfterConnection,
                              SystemStage::parallel()
                                 .with_run_criteria(should_connect))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::Disconnection,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_disconnect))
            .add_stage_after(Stage::Disconnection,
                              PrivateStage::AfterDisconnection,
                              SystemStage::parallel()
                                 .with_run_criteria(should_disconnect))
            // frame //
            .add_stage_after(CoreStage::PostUpdate,
                              Stage::PreFrame,
                              SystemStage::single_threaded())
            .add_stage_after(Stage::PreFrame,
                              Stage::Frame,
                              SystemStage::single_threaded())
            .add_stage_after(Stage::Frame,
                              Stage::PostFrame,
                              SystemStage::single_threaded())
            // tick //
            .add_stage_after(Stage::PostFrame,
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
            .add_system_to_stage(PrivateStage::AfterConnection,
                                 finish_connect)
            .add_system_to_stage(PrivateStage::AfterDisconnection,
                                 finish_disconnect)
            .add_system_to_stage(PrivateStage::AfterTick,
                                 finish_tick);
    }
}
