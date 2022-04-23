use std::{marker::PhantomData, ops::DerefMut, sync::Mutex};

use bevy_app::{App, CoreStage, Plugin as PluginType};
use bevy_ecs::{entity::Entity, schedule::SystemStage, system::IntoExclusiveSystem};

use naia_client::{
    shared::{ChannelIndex, Protocolize, SharedConfig},
    Client, ClientConfig,
};

use naia_bevy_shared::WorldData;

use crate::systems::should_receive;

use super::{
    events::{
        DespawnEntityEvent, InsertComponentEvent, MessageEvent, RemoveComponentEvent,
        SpawnEntityEvent, UpdateComponentEvent,
    },
    resource::ClientResource,
    stage::{PrivateStage, Stage},
    systems::{
        before_receive_events, finish_connect, finish_disconnect, finish_tick, should_connect,
        should_disconnect, should_tick,
    },
};

struct PluginConfig<C: ChannelIndex> {
    client_config: ClientConfig,
    shared_config: SharedConfig<C>,
}

impl<C: ChannelIndex> PluginConfig<C> {
    pub fn new(client_config: ClientConfig, shared_config: SharedConfig<C>) -> Self {
        PluginConfig {
            client_config,
            shared_config,
        }
    }
}

pub struct Plugin<P: Protocolize, C: ChannelIndex> {
    config: Mutex<Option<PluginConfig<C>>>,
    phantom_p: PhantomData<P>,
}

impl<P: Protocolize, C: ChannelIndex> Plugin<P, C> {
    pub fn new(client_config: ClientConfig, shared_config: SharedConfig<C>) -> Self {
        let config = PluginConfig::new(client_config, shared_config);
        Plugin {
            config: Mutex::new(Some(config)),
            phantom_p: PhantomData,
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> PluginType for Plugin<P, C> {
    fn build(&self, app: &mut App) {
        let config = self.config.lock().unwrap().deref_mut().take().unwrap();
        let client = Client::<P, Entity, C>::new(&config.client_config, &config.shared_config);

        app
            // RESOURCES //
            .insert_resource(client)
            .init_resource::<ClientResource>()
            .init_resource::<WorldData<P>>()
            // EVENTS //
            .add_event::<SpawnEntityEvent>()
            .add_event::<DespawnEntityEvent>()
            .add_event::<InsertComponentEvent<P::Kind>>()
            .add_event::<UpdateComponentEvent<P::Kind>>()
            .add_event::<RemoveComponentEvent<P>>()
            .add_event::<MessageEvent<P, C>>()
            // STAGES //
            // events //
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
                PrivateStage::BeforeReceiveEvents,
                Stage::Connection,
                SystemStage::single_threaded().with_run_criteria(should_connect),
            )
            .add_stage_after(
                Stage::Connection,
                PrivateStage::AfterConnection,
                SystemStage::parallel().with_run_criteria(should_connect),
            )
            .add_stage_after(
                PrivateStage::BeforeReceiveEvents,
                Stage::Disconnection,
                SystemStage::single_threaded().with_run_criteria(should_disconnect),
            )
            .add_stage_after(
                Stage::Disconnection,
                PrivateStage::AfterDisconnection,
                SystemStage::parallel().with_run_criteria(should_disconnect),
            )
            // frame //
            .add_stage_after(
                CoreStage::PostUpdate,
                Stage::PreFrame,
                SystemStage::single_threaded(),
            )
            .add_stage_after(
                Stage::PreFrame,
                Stage::Frame,
                SystemStage::single_threaded(),
            )
            .add_stage_after(
                Stage::Frame,
                Stage::PostFrame,
                SystemStage::single_threaded(),
            )
            // tick //
            .add_stage_after(
                Stage::PostFrame,
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
            .add_system_to_stage(PrivateStage::AfterConnection, finish_connect)
            .add_system_to_stage(PrivateStage::AfterDisconnection, finish_disconnect)
            .add_system_to_stage(PrivateStage::AfterTick, finish_tick);
    }
}
