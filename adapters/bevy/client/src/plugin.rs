use std::{net::SocketAddr, ops::DerefMut, sync::Mutex};

use bevy::{
    app::{App, CoreStage, Plugin as PluginType},
    ecs::schedule::SystemStage,
    prelude::*,
};

use naia_client::{Client, ClientConfig, ProtocolType, Replicate, SharedConfig};

use crate::systems::should_do_io;
use naia_bevy_shared::WorldData;

use super::{
    events::{
        DespawnEntityEvent, DisownEntityEvent, InsertComponentEvent, MessageEvent, NewCommandEvent,
        OwnEntityEvent, RemoveComponentEvent, ReplayCommandEvent, RewindEntityEvent,
        SpawnEntityEvent, UpdateComponentEvent,
    },
    resource::ClientResource,
    stage::{PrivateStage, Stage},
    systems::{
        before_receive_events, finish_connect, finish_disconnect, finish_tick, should_connect,
        should_disconnect, should_tick,
    },
};

struct PluginConfig<P: ProtocolType, R: Replicate<P>> {
    client_config: ClientConfig,
    shared_config: SharedConfig<P>,
    server_address: SocketAddr,
    auth: Option<R>,
}

impl<P: ProtocolType, R: Replicate<P>> PluginConfig<P, R> {
    pub fn new(
        client_config: ClientConfig,
        shared_config: SharedConfig<P>,
        server_address: SocketAddr,
        auth: Option<R>,
    ) -> Self {
        PluginConfig {
            client_config,
            shared_config,
            server_address,
            auth,
        }
    }
}

pub struct Plugin<P: ProtocolType, R: Replicate<P>> {
    config: Mutex<Option<PluginConfig<P, R>>>,
}

impl<P: ProtocolType, R: Replicate<P>> Plugin<P, R> {
    pub fn new(
        client_config: ClientConfig,
        shared_config: SharedConfig<P>,
        server_address: SocketAddr,
        auth_ref: Option<R>,
    ) -> Self {
        let config = PluginConfig::new(client_config, shared_config, server_address, auth_ref);
        return Plugin {
            config: Mutex::new(Some(config)),
        };
    }
}

impl<P: ProtocolType, R: Replicate<P>> PluginType for Plugin<P, R> {
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let mut client = Client::<P, Entity>::new(config.client_config, config.shared_config);

        app
        // RESOURCES //
            .insert_resource(client)
            .insert_resource(ClientResource::new())
            .insert_resource(WorldData::<P>::new())
        // EVENTS //
            .add_event::<SpawnEntityEvent<P>>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<OwnEntityEvent>()
            .add_event::<DisownEntityEvent>()
            .add_event::<RewindEntityEvent>()
            .add_event::<InsertComponentEvent<P>>()
            .add_event::<UpdateComponentEvent<P>>()
            .add_event::<RemoveComponentEvent<P>>()
            .add_event::<MessageEvent<P>>()
            .add_event::<NewCommandEvent<P>>()
            .add_event::<ReplayCommandEvent<P>>()
        // STAGES //
            // events //
            .add_stage_before(CoreStage::PreUpdate,
                              PrivateStage::BeforeReceiveEvents,
                              SystemStage::single_threaded()
                                  .with_run_criteria(should_do_io::<P>.system()))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::ReceiveEvents,
                              SystemStage::single_threaded())
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::Connection,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_connect.system()))
            .add_stage_after(Stage::Connection,
                              PrivateStage::AfterConnection,
                              SystemStage::parallel()
                                 .with_run_criteria(should_connect.system()))
            .add_stage_after(PrivateStage::BeforeReceiveEvents,
                              Stage::Disconnection,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_disconnect.system()))
            .add_stage_after(Stage::Disconnection,
                              PrivateStage::AfterDisconnection,
                              SystemStage::parallel()
                                 .with_run_criteria(should_disconnect.system()))
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
                                 .with_run_criteria(should_tick.system()))
            .add_stage_after(Stage::Tick,
                              PrivateStage::AfterTick,
                              SystemStage::parallel()
                                 .with_run_criteria(should_tick.system()))
            // SYSTEMS //
            .add_system_to_stage(PrivateStage::BeforeReceiveEvents,
                                 before_receive_events::<P>.exclusive_system())
            .add_system_to_stage(PrivateStage::AfterConnection,
                                 finish_connect.system())
            .add_system_to_stage(PrivateStage::AfterDisconnection,
                                 finish_disconnect.system())
            .add_system_to_stage(PrivateStage::AfterTick,
                                 finish_tick.system());
    }
}
