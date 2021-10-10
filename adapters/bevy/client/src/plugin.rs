use std::{net::SocketAddr, ops::DerefMut, sync::Mutex};

use bevy::app::{AppBuilder, Plugin as PluginType};

use naia_client::{ProtocolType, Client, ClientConfig, SharedConfig, ImplRef};

use naia_bevy_shared::Entity;

use super::ticker::Ticker;

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
            auth_ref
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
            .insert_resource(Ticker::new());
    }
}
