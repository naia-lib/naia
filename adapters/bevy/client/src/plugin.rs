use std::{marker::PhantomData, ops::DerefMut, sync::Mutex};

use bevy_app::{App, Plugin as PluginType, Startup, Update};
use bevy_ecs::{entity::Entity, schedule::IntoScheduleConfigs};

use naia_bevy_shared::{
    HandleTickEvents, HandleWorldEvents, HostSyncChangeTracking, HostSyncOwnedAddedTracking,
    ProcessPackets, Protocol, ReceivePackets, SendPackets, SharedPlugin, TranslateTickEvents,
    TranslateWorldEvents, WorldData, WorldToHostSync, WorldUpdate,
};
use naia_client::{Client, ClientConfig};

use crate::{component_event_registry::ComponentEventRegistry, events::RequestEvents};

use super::{
    client::ClientWrapper,
    events::{
        ClientTickEvent, ConnectEvent, DespawnEntityEvent, DisconnectEvent, EntityAuthDeniedEvent,
        EntityAuthGrantedEvent, EntityAuthResetEvent, ErrorEvent, MessageEvents,
        PublishEntityEvent, RejectEvent, ServerTickEvent, SpawnEntityEvent, UnpublishEntityEvent,
    },
    systems::{
        process_packets, receive_packets, send_packets, send_packets_init, translate_tick_events,
        translate_world_events, world_to_host_sync,
    },
};

struct PluginConfig {
    client_config: ClientConfig,
    protocol: Protocol,
}

impl PluginConfig {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        Self {
            client_config,
            protocol,
        }
    }
}

pub struct Plugin<T> {
    config: Mutex<Option<PluginConfig>>,
    phantom_t: PhantomData<T>,
}

impl<T> Plugin<T> {
    pub fn new(client_config: ClientConfig, protocol: Protocol) -> Self {
        let config = PluginConfig::new(client_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
            phantom_t: PhantomData,
        }
    }
}

impl<T: Sync + Send + 'static> PluginType for Plugin<T> {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().unwrap().deref_mut().take().unwrap();

        let mut world_data = config.protocol.take_world_data();
        world_data.add_systems(app);

        if let Some(old_world_data) = app.world_mut().remove_resource::<WorldData>() {
            world_data.merge(old_world_data);
        }

        app.insert_resource(world_data);

        let client = Client::<Entity>::new(config.client_config, config.protocol.into());
        let client = ClientWrapper::<T>::new(client);

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<T>::new())
            // RESOURCES //
            .insert_resource(client)
            .init_resource::<ComponentEventRegistry<T>>()
            // EVENTS //
            .add_event::<ConnectEvent<T>>()
            .add_event::<DisconnectEvent<T>>()
            .add_event::<RejectEvent<T>>()
            .add_event::<ErrorEvent<T>>()
            .add_event::<MessageEvents<T>>()
            .add_event::<RequestEvents<T>>()
            .add_event::<ClientTickEvent<T>>()
            .add_event::<ServerTickEvent<T>>()
            .add_event::<SpawnEntityEvent<T>>()
            .add_event::<DespawnEntityEvent<T>>()
            .add_event::<PublishEntityEvent<T>>()
            .add_event::<UnpublishEntityEvent<T>>()
            .add_event::<EntityAuthGrantedEvent<T>>()
            .add_event::<EntityAuthDeniedEvent<T>>()
            .add_event::<EntityAuthResetEvent<T>>()
            // SYSTEM SETS //
            .configure_sets(Update, ReceivePackets.before(TranslateTickEvents))
            .configure_sets(Update, TranslateTickEvents.before(HandleTickEvents))
            .configure_sets(Update, HandleTickEvents.before(ProcessPackets))
            .configure_sets(Update, ProcessPackets.before(TranslateWorldEvents))
            .configure_sets(Update, TranslateWorldEvents.before(HandleWorldEvents))
            .configure_sets(Update, HandleWorldEvents.before(WorldUpdate))
            .configure_sets(Update, WorldUpdate.before(HostSyncOwnedAddedTracking))
            .configure_sets(
                Update,
                HostSyncOwnedAddedTracking.before(HostSyncChangeTracking),
            )
            .configure_sets(Update, HostSyncChangeTracking.before(WorldToHostSync))
            .configure_sets(Update, WorldToHostSync.before(SendPackets))
            // SYSTEMS //
            .add_systems(Update, receive_packets::<T>.in_set(ReceivePackets))
            .add_systems(
                Update,
                translate_tick_events::<T>.in_set(TranslateTickEvents),
            )
            .add_systems(Update, process_packets::<T>.in_set(ProcessPackets))
            .add_systems(
                Update,
                translate_world_events::<T>.in_set(TranslateWorldEvents),
            )
            .add_systems(Update, world_to_host_sync::<T>.in_set(WorldToHostSync))
            .add_systems(Startup, send_packets_init::<T>)
            .add_systems(Update, send_packets::<T>.in_set(SendPackets));
    }
}
