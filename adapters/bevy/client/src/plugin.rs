use parking_lot::Mutex;
use std::{marker::PhantomData, ops::DerefMut};

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

/// Bevy plugin that wires naia's client replication into a Bevy `App`.
///
/// `T` is the Bevy `Entity` type (pass `bevy_ecs::entity::Entity`).
///
/// Registers the [`Client`] resource, adds all required systems, and emits
/// naia events as standard Bevy events so they can be consumed in any system.
///
/// # Scheduled systems
///
/// The plugin schedules the following in `Update` (in dependency order):
///
/// 1. `receive_packets` — reads datagrams from the socket
/// 2. `process_packets` — decodes server-replicated state changes
/// 3. `translate_world_events` — converts naia events to Bevy events
/// 4. `translate_tick_events` — emits tick Bevy events
/// 5. `world_to_host_sync` — syncs Bevy world changes back to naia
/// 6. `send_packets` — serialises and flushes outbound packets
pub struct Plugin<T> {
    config: Mutex<Option<PluginConfig>>,
    phantom_t: PhantomData<T>,
}

impl<T> Plugin<T> {
    /// Creates the plugin with the given client configuration and protocol.
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
        let mut config = self.config.lock().deref_mut().take().unwrap();

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
            .add_message::<ConnectEvent<T>>()
            .add_message::<DisconnectEvent<T>>()
            .add_message::<RejectEvent<T>>()
            .add_message::<ErrorEvent<T>>()
            .add_message::<MessageEvents<T>>()
            .add_message::<RequestEvents<T>>()
            .add_message::<ClientTickEvent<T>>()
            .add_message::<ServerTickEvent<T>>()
            .add_message::<SpawnEntityEvent<T>>()
            .add_message::<DespawnEntityEvent<T>>()
            .add_message::<PublishEntityEvent<T>>()
            .add_message::<UnpublishEntityEvent<T>>()
            .add_message::<EntityAuthGrantedEvent<T>>()
            .add_message::<EntityAuthDeniedEvent<T>>()
            .add_message::<EntityAuthResetEvent<T>>()
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
