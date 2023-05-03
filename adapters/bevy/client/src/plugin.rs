use std::{ops::DerefMut, sync::Mutex};

use bevy_app::{App, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::IntoSystemConfig};

use naia_bevy_shared::{BeforeReceiveEvents, Protocol, SharedPlugin};
use naia_client::{Client, ClientConfig};

use super::{
    events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent,
        EntityDisableDelegationEvent, EntityEnableDelegationEvent, ErrorEvent,
        InsertComponentEvents, MessageEvents, PublishEntityEvent, RejectEvent,
        RemoveComponentEvents, ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent,
        UpdateComponentEvents,
    },
    systems::before_receive_events,
};

struct PluginConfig {
    client_config: ClientConfig,
    protocol: Protocol,
}

impl PluginConfig {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        PluginConfig {
            client_config,
            protocol,
        }
    }
}

pub struct Plugin {
    config: Mutex<Option<PluginConfig>>,
}

impl Plugin {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        let config = PluginConfig::new(client_config, protocol);
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

        let client = Client::<Entity>::new(config.client_config, config.protocol.into());

        app
            // SHARED PLUGIN //
            .add_plugin(SharedPlugin)
            // RESOURCES //
            .insert_resource(client)
            // EVENTS //
            .add_event::<ConnectEvent>()
            .add_event::<DisconnectEvent>()
            .add_event::<RejectEvent>()
            .add_event::<ErrorEvent>()
            .add_event::<ClientTickEvent>()
            .add_event::<ServerTickEvent>()
            .add_event::<MessageEvents>()
            .add_event::<SpawnEntityEvent>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<PublishEntityEvent>()
            .add_event::<UnpublishEntityEvent>()
            .add_event::<EntityEnableDelegationEvent>()
            .add_event::<EntityDisableDelegationEvent>()
            .add_event::<InsertComponentEvents>()
            .add_event::<UpdateComponentEvents>()
            .add_event::<RemoveComponentEvents>()
            // SYSTEMS //
            .add_system(before_receive_events.in_set(BeforeReceiveEvents));
    }
}
