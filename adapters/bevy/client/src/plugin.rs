use std::{ops::DerefMut, sync::Mutex};

use bevy_app::{App, CoreStage, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::SystemStage};

use naia_client::{Client, ClientConfig};

use naia_bevy_shared::Protocol;

use super::{
    events::{
        ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, InsertComponentEvents,
        MessageEvents, RejectEvent, RemoveComponentEvents, SpawnEntityEvent, UpdateComponentEvents, TickEvent
    },
    stage::{PrivateStage, Stage},
    systems::{before_receive_events, should_receive},
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

        let world_data = config.protocol.world_data();
        app.insert_resource(world_data);

        let client = Client::<Entity>::new(config.client_config, config.protocol.into());

        app
            // RESOURCES //
            .insert_resource(client)
            // EVENTS //
            .add_event::<ConnectEvent>()
            .add_event::<DisconnectEvent>()
            .add_event::<RejectEvent>()
            .add_event::<ErrorEvent>()
            .add_event::<TickEvent>()
            .add_event::<MessageEvents>()
            .add_event::<SpawnEntityEvent>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<InsertComponentEvents>()
            .add_event::<UpdateComponentEvents>()
            .add_event::<RemoveComponentEvents>()
            // STAGES //
            // events //
            .add_stage_before(
                CoreStage::PreUpdate,
                PrivateStage::BeforeReceiveEvents,
                SystemStage::single_threaded().with_run_criteria(should_receive),
            )
            .add_stage_after(
                PrivateStage::BeforeReceiveEvents,
                Stage::ReceiveEvents,
                SystemStage::single_threaded().with_run_criteria(should_receive),
            )
            // SYSTEMS //
            .add_system_to_stage(PrivateStage::BeforeReceiveEvents, before_receive_events);
    }
}
