use std::{ops::DerefMut, sync::Mutex};

use bevy_app::{App, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::IntoSystemConfig};

use naia_bevy_shared::{BeforeReceiveEvents, Protocol, SharedPlugin};
use naia_server::{Server, ServerConfig};

use super::{
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, RemoveComponentEvents, SpawnEntityEvent, TickEvent,
        UpdateComponentEvents,
    },
    systems::before_receive_events,
};

struct PluginConfig {
    server_config: ServerConfig,
    protocol: Protocol,
}

impl PluginConfig {
    pub fn new(server_config: ServerConfig, protocol: Protocol) -> Self {
        PluginConfig {
            server_config,
            protocol,
        }
    }
}

pub struct Plugin {
    config: Mutex<Option<PluginConfig>>,
}

impl Plugin {
    pub fn new(server_config: ServerConfig, protocol: Protocol) -> Self {
        let config = PluginConfig::new(server_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
        }
    }
}

impl PluginType for Plugin {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().unwrap().deref_mut().take().unwrap();

        let world_data = config.protocol.take_world_data();
        world_data.add_systems(app);
        app.insert_resource(world_data);

        let server = Server::<Entity>::new(config.server_config, config.protocol.into());

        app
            // SHARED PLUGIN //
            .add_plugin(SharedPlugin)
            // RESOURCES //
            .insert_resource(server)
            // EVENTS //
            .add_event::<ConnectEvent>()
            .add_event::<DisconnectEvent>()
            .add_event::<ErrorEvent>()
            .add_event::<TickEvent>()
            .add_event::<MessageEvents>()
            .add_event::<AuthEvents>()
            .add_event::<SpawnEntityEvent>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<InsertComponentEvents>()
            .add_event::<UpdateComponentEvents>()
            .add_event::<RemoveComponentEvents>()
            // SYSTEMS //
            .add_system(before_receive_events.in_set(BeforeReceiveEvents));
    }
}
