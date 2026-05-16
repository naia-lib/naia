use parking_lot::Mutex;
use std::{ops::DerefMut};

use bevy_app::{App, Plugin as PluginType, Startup, Update};
use bevy_ecs::{entity::Entity, prelude::ApplyDeferred, schedule::IntoScheduleConfigs};

use naia_bevy_shared::{
    HandleTickEvents, HandleWorldEvents, HostSyncChangeTracking, HostSyncOwnedAddedTracking,
    ProcessPackets, Protocol, ReceivePackets, SendPackets, SharedPlugin, TranslateTickEvents,
    TranslateWorldEvents, WorldToHostSync, WorldUpdate,
};
use naia_server::{shared::Protocol as NaiaProtocol, Server, ServerConfig, WorldServer};

use super::{
    component_event_registry::ComponentEventRegistry,
    events::{
        AuthEvents, ConnectEvent, DespawnEntityEvent, DisconnectEvent, ErrorEvent, MessageEvents,
        PublishEntityEvent, RequestEvents, SpawnEntityEvent, TickEvent, UnpublishEntityEvent,
    },
    server::ServerImpl,
    systems::{
        process_packets, receive_packets, send_packets, send_packets_init, translate_tick_events,
        translate_world_events, world_to_host_sync,
    },
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

#[derive(Clone)]
pub struct Singleton;

/// Bevy plugin that wires naia's server replication into a Bevy `App`.
///
/// Registers the [`Server`] resource, adds all required systems, and emits
/// naia events as standard Bevy events so they can be consumed in any system.
///
/// # Scheduled systems
///
/// The plugin schedules the following in `Update` (in dependency order):
///
/// 1. `receive_packets` — reads datagrams from the socket
/// 2. `process_packets` — decodes and applies entity/component changes
/// 3. `translate_world_events` — converts naia events to Bevy events
/// 4. `translate_tick_events` — emits [`TickEvent`] Bevy events
/// 5. `world_to_host_sync` — syncs Bevy world changes into naia
/// 6. `send_packets` — serialises and flushes outbound packets
///
/// [`TickEvent`]: crate::events::TickEvent
pub struct Plugin {
    config: Mutex<Option<PluginConfig>>,
    world_only: bool,
    /// Pipeline mode (Phase 4 capacity uplift): when `true`, the coordinator
    /// drives recv/translate/send phases directly via `RecvHandle`/`SendHandle`
    /// and `apply_receive_output`. The plugin therefore skips registering
    /// `receive_packets`, `process_packets`, `translate_world_events`,
    /// `translate_tick_events`, `send_packets_init`, and `send_packets` in
    /// `Update`. Naia's sync systems (`world_to_host_sync`,
    /// `HostSyncOwnedAddedTracking`, `HostSyncChangeTracking`) remain in
    /// `Update` — the coordinator invokes `run_schedule(Update)` between the
    /// `PhysicsSyncSchedule` and the send kick.
    pipeline: bool,
}

impl Plugin {
    /// Creates the plugin with the given server configuration and protocol.
    pub fn new(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, false, false)
    }

    /// World-only variant. Skips full `Server` setup (auth, accept_connection,
    /// etc.) and registers a `WorldServer` instead. Used by services that
    /// proxy connections (e.g., the game cell behind a session server).
    pub fn world_only(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, false)
    }

    /// Pipeline + world-only variant (Phase 4 capacity uplift). Builds the
    /// same `WorldServer` as `world_only`, but skips the recv/translate/send
    /// `Update` systems — those phases are driven explicitly by the
    /// pipeline coordinator via `RecvHandle`/`SendHandle`. Naia's host-sync
    /// systems (`HostSyncOwnedAddedTracking`, `HostSyncChangeTracking`,
    /// `WorldToHostSync`) remain in `Update`.
    pub fn world_only_pipeline(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, true)
    }

    fn new_impl(
        server_config: ServerConfig,
        protocol: Protocol,
        world_only: bool,
        pipeline: bool,
    ) -> Self {
        let config = PluginConfig::new(server_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
            world_only,
            pipeline,
        }
    }
}

impl PluginType for Plugin {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().deref_mut().take().unwrap();

        let world_data = config.protocol.take_world_data();
        world_data.add_systems(app);
        app.insert_resource(world_data);

        let server_impl = if !self.world_only {
            let server = Server::<Entity>::new(config.server_config, config.protocol.into());
            ServerImpl::full(server)
        } else {
            let protocol: NaiaProtocol = config.protocol.into();
            let server = WorldServer::<Entity>::new(config.server_config, protocol);
            ServerImpl::world_only(server)
        };

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<Singleton>::new())
            // RESOURCES //
            .insert_resource(server_impl)
            .init_resource::<ComponentEventRegistry>()
            // EVENTS //
            .add_message::<ConnectEvent>()
            .add_message::<DisconnectEvent>()
            .add_message::<ErrorEvent>()
            .add_message::<TickEvent>()
            .add_message::<MessageEvents>()
            .add_message::<RequestEvents>()
            .add_message::<AuthEvents>()
            .add_message::<SpawnEntityEvent>()
            .add_message::<DespawnEntityEvent>()
            .add_message::<PublishEntityEvent>()
            .add_message::<UnpublishEntityEvent>()
            // SYSTEM SETS //
            .configure_sets(Update, ReceivePackets.before(ProcessPackets))
            .configure_sets(Update, ProcessPackets.before(TranslateWorldEvents))
            .configure_sets(Update, TranslateWorldEvents.before(HandleWorldEvents))
            .configure_sets(Update, HandleWorldEvents.before(TranslateTickEvents))
            .configure_sets(Update, TranslateTickEvents.before(HandleTickEvents))
            .configure_sets(Update, HandleTickEvents.before(WorldUpdate))
            .configure_sets(Update, WorldUpdate.before(HostSyncOwnedAddedTracking))
            .configure_sets(
                Update,
                HostSyncOwnedAddedTracking.before(HostSyncChangeTracking),
            )
            // Flush deferred Bevy commands (e.g. component inserts from HandleWorldEvents)
            // before naia's change-detection systems run so they see the new components.
            .add_systems(Update, ApplyDeferred.in_set(HostSyncOwnedAddedTracking))
            .configure_sets(Update, HostSyncChangeTracking.before(WorldToHostSync))
            .configure_sets(Update, WorldToHostSync.before(SendPackets))
            // SYSTEMS //
            .add_systems(Update, world_to_host_sync.in_set(WorldToHostSync));

        // Recv/translate/send systems are driven by the pipeline coordinator
        // in pipeline mode — skip registering them in `Update`.
        if !self.pipeline {
            app
                .add_systems(Update, receive_packets.in_set(ReceivePackets))
                .add_systems(Update, process_packets.in_set(ProcessPackets))
                .add_systems(Update, translate_world_events.in_set(TranslateWorldEvents))
                .add_systems(Update, translate_tick_events.in_set(TranslateTickEvents))
                .add_systems(Startup, send_packets_init)
                .add_systems(Update, send_packets.in_set(SendPackets));
        }
    }
}
