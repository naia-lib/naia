use std::{marker::PhantomData, ops::DerefMut, sync::Mutex};

use bevy_app::{App, CoreStage, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::SystemStage, system::IntoExclusiveSystem};

use naia_server::{
    shared::{ChannelIndex, Protocolize, SharedConfig},
    Server, ServerConfig,
};

use naia_bevy_shared::WorldData;

use super::{
    events::{AuthorizationEvent, ConnectionEvent, DisconnectionEvent, MessageEvent},
    resource::ServerResource,
    stage::{PrivateStage, Stage},
    systems::{before_receive_events, finish_tick, should_receive, should_tick},
};

struct PluginConfig<C: ChannelIndex> {
    server_config: ServerConfig,
    shared_config: SharedConfig<C>,
}

impl<C: ChannelIndex> PluginConfig<C> {
    pub fn new(server_config: ServerConfig, shared_config: SharedConfig<C>) -> Self {
        PluginConfig {
            server_config,
            shared_config,
        }
    }
}

pub struct Plugin<P: Protocolize, C: ChannelIndex> {
    config: Mutex<Option<PluginConfig<C>>>,
    phantom_p: PhantomData<P>,
}

// unsafe impl<P: Protocolize, C: ChannelIndex> Send for Plugin<P, C> {}
// unsafe impl<P: Protocolize, C: ChannelIndex> Sync for Plugin<P, C> {}

impl<P: Protocolize, C: ChannelIndex> Plugin<P, C> {
    pub fn new(server_config: ServerConfig, shared_config: SharedConfig<C>) -> Self {
        let config = PluginConfig::new(server_config, shared_config);
        Self {
            config: Mutex::new(Some(config)),
            phantom_p: PhantomData,
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PluginType for Plugin<P, C> {
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let server = Server::<P, Entity, C>::new(&config.server_config, &config.shared_config);

        app
            // RESOURCES //
            .insert_resource(server)
            .init_resource::<ServerResource>()
            .init_resource::<WorldData<P>>()
            // EVENTS //
            .add_event::<AuthorizationEvent<P>>()
            .add_event::<ConnectionEvent>()
            .add_event::<DisconnectionEvent>()
            .add_event::<MessageEvent<P, C>>()
            // STAGES //
            .add_stage_before(
                CoreStage::PreUpdate,
                PrivateStage::BeforeReceiveEvents,
                SystemStage::single_threaded().with_run_criteria(should_receive::<P, C>),
            )
            .add_stage_after(
                PrivateStage::BeforeReceiveEvents,
                Stage::ReceiveEvents,
                SystemStage::single_threaded().with_run_criteria(should_receive::<P, C>),
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
            .add_system_to_stage(
                PrivateStage::BeforeReceiveEvents,
                before_receive_events::<P, C>.exclusive_system(),
            )
            .add_system_to_stage(PrivateStage::AfterTick, finish_tick);
    }
}
