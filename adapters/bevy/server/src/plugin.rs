use std::{ops::DerefMut, sync::Mutex};

use bevy_app::{App, CoreStage, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::SystemStage};

use naia_server::{Server, ServerConfig};

use naia_bevy_shared::Protocol;

use super::{
    events::{AuthEvents, ConnectEvent, DisconnectEvent, ErrorEvent, MessageEvents},
    resource::ServerResource,
    stage::{PrivateStage, Stage},
    systems::{before_receive_events, finish_tick, should_receive, should_tick},
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

        let world_data = config.protocol.world_data();
        app.insert_resource(world_data);

        let server = Server::<Entity>::new(config.server_config, config.protocol.into());

        app
            // RESOURCES //
            .insert_resource(server)
            .init_resource::<ServerResource>()
            // EVENTS //
            .add_event::<ConnectEvent>()
            .add_event::<DisconnectEvent>()
            .add_event::<AuthEvents>()
            .add_event::<MessageEvents>()
            .add_event::<ErrorEvent>()
            // STAGES //
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
            .add_stage_after(
                CoreStage::PostUpdate,
                Stage::Tick,
                SystemStage::single_threaded().with_run_criteria(should_tick),
            )
            .add_stage_after(
                Stage::Tick,
                PrivateStage::AfterTick,
                SystemStage::parallel().with_run_criteria(should_tick),
            )
            // SYSTEMS //
            .add_system_to_stage(PrivateStage::BeforeReceiveEvents, before_receive_events)
            .add_system_to_stage(PrivateStage::AfterTick, finish_tick);
    }
}
