use std::{net::SocketAddr, ops::DerefMut, sync::Mutex};

use bevy::{app::{AppBuilder, Plugin as PluginType, CoreStage}, ecs::schedule::SystemStage};

use naia_client::{Client, ClientConfig, ImplRef, ProtocolType, SharedConfig};

use naia_bevy_shared::Entity;

use super::{resource::ClientResource, stage::Stage, systems::receive_events};

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
            .insert_resource(ClientResource::new())
        // STAGES //
            .add_stage_before(CoreStage::PreUpdate, Stage::ReceiveEvents, SystemStage::single_threaded())
        // SYSTEMS //
            .add_system_to_stage(Stage::ReceiveEvents, receive_events.exclusive_system());
    }
}
