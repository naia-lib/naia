use std::{net::SocketAddr, ops::DerefMut, sync::Mutex};

use bevy::{
    app::{AppBuilder, CoreStage, Plugin as PluginType},
    ecs::schedule::SystemStage,
    prelude::*,
};

use naia_client::{Client, ClientConfig, ImplRef, ProtocolType, SharedConfig};

use naia_bevy_shared::{
    tick::{finish_tick, should_tick, Ticker},
    Entity, PrivateStage, Stage, WorldData,
};

use super::{resource::ClientResource, stage::ClientStage, systems::before_receive_events};

struct PluginConfig<P: ProtocolType, R: ImplRef<P>> {
    client_config: ClientConfig,
    shared_config: SharedConfig<P>,
    server_address: SocketAddr,
    auth_ref: Option<R>,
}

impl<P: ProtocolType, R: ImplRef<P>> PluginConfig<P, R> {
    pub fn new(
        client_config: ClientConfig,
        shared_config: SharedConfig<P>,
        server_address: SocketAddr,
        auth_ref: Option<R>,
    ) -> Self {
        PluginConfig {
            client_config,
            shared_config,
            server_address,
            auth_ref,
        }
    }
}

pub struct Plugin<P: ProtocolType, R: ImplRef<P>> {
    config: Mutex<Option<PluginConfig<P, R>>>,
}

impl<P: ProtocolType, R: ImplRef<P>> Plugin<P, R> {
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

impl<P: ProtocolType, R: ImplRef<P>> PluginType for Plugin<P, R> {
    fn build(&self, app: &mut AppBuilder) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let mut client = Client::<P, Entity>::new(config.client_config, config.shared_config);
        client.connect(config.server_address, config.auth_ref);

        app
        // RESOURCES //
            .insert_resource(client)
            .insert_resource(ClientResource::<P>::new())
            .insert_resource(Ticker::new())
            .insert_resource(WorldData::<P>::new())
        // STAGES //
            .add_stage_before(CoreStage::PreUpdate,
                              ClientStage::BeforeReceiveEvents,
                              SystemStage::parallel())
            .add_stage_after(ClientStage::BeforeReceiveEvents,
                              Stage::ReceiveEvents,
                              SystemStage::single_threaded())
            .add_stage_after(CoreStage::PostUpdate,
                              Stage::PreFrame,
                              SystemStage::single_threaded())
            .add_stage_after(Stage::PreFrame,
                              Stage::Frame,
                              SystemStage::single_threaded())
            .add_stage_after(Stage::Frame,
                              Stage::PostFrame,
                              SystemStage::single_threaded())
            .add_stage_after(Stage::PostFrame,
                              Stage::Tick,
                              SystemStage::single_threaded()
                                 .with_run_criteria(should_tick.system()))
            .add_stage_after(Stage::Tick,
                              PrivateStage::AfterTick,
                              SystemStage::parallel()
                                 .with_run_criteria(should_tick.system()))
        // SYSTEMS //
            .add_system_to_stage(ClientStage::BeforeReceiveEvents,
                                 before_receive_events::<P>.exclusive_system())
            .add_system_to_stage(PrivateStage::AfterTick,
                                 finish_tick.system());
    }
}
