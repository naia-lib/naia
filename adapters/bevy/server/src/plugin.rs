use std::{ops::DerefMut, sync::Mutex};

use bevy_app::{App, Last, Plugin as PluginType, Startup, Update};
use bevy_ecs::{schedule::IntoSystemConfigs};

use naia_bevy_shared::{BeforeReceiveEvents, Protocol, SendPackets, SharedPlugin};
use naia_server::{Server, ServerConfig};

use super::{
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RemoveComponentEvents,
        RequestEvents, SpawnEntityEvent, TickEvent, UnpublishEntityEvent, UpdateComponentEvents,
    },
    server::ServerWrapper,
    systems::{main_world_before_receive_events, send_packets, send_packets_init},
    world_entity::{WorldId, WorldEntity},
};

struct PluginConfig {
    server_config: ServerConfig,
    protocol: Protocol,
}

impl PluginConfig {
    pub fn new(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self {
            server_config,
            protocol,
        }
    }
}

#[derive(Clone)]
pub struct Singleton;

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

        let server = Server::<WorldEntity>::new(config.server_config, config.protocol.into());
        let server = ServerWrapper::main(server);

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<Singleton>::new())
            // RESOURCES //
            .insert_resource(server)
            .insert_resource(WorldId::main())
            // EVENTS //
            .add_event::<ConnectEvent>()
            .add_event::<DisconnectEvent>()
            .add_event::<ErrorEvent>()
            .add_event::<TickEvent>()
            .add_event::<MessageEvents>()
            .add_event::<RequestEvents>()
            .add_event::<AuthEvents>()
            .add_event::<SpawnEntityEvent>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<PublishEntityEvent>()
            .add_event::<UnpublishEntityEvent>()
            .add_event::<InsertComponentEvents>()
            .add_event::<UpdateComponentEvents>()
            .add_event::<RemoveComponentEvents>()
            // SYSTEM SETS //
            .configure_sets(Last, SendPackets)
            // SYSTEMS //
            .add_systems(Update, main_world_before_receive_events.in_set(BeforeReceiveEvents))
            .add_systems(Startup, send_packets_init)
            .add_systems(Update, send_packets.in_set(SendPackets));
    }
}
