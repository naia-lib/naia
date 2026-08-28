use parking_lot::Mutex;
use std::ops::DerefMut;

use bevy_app::{App, Plugin as PluginType, Startup, Update};
use bevy_ecs::{
    entity::Entity,
    prelude::ApplyDeferred,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
};

use naia_bevy_shared::{
    on_despawn, on_host_owned_added, HandleTickEvents, HandleWorldEvents, HostSyncChangeTracking,
    HostSyncOwnedAddedTracking, ProcessPackets, Protocol, ReceivePackets, SendPackets,
    SharedPlugin, TranslateTickEvents, TranslateWorldEvents, WorldToHostSync, WorldUpdate,
};
use naia_server::{
    shared::Protocol as NaiaProtocol, Server, ServerConfig, ServerMode, WorldServer,
};

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

/// Explicit configuration for the Bevy server plugin.
pub struct ServerPluginConfig {
    pub server_config: ServerConfig,
    pub protocol: Protocol,
    pub topology: Topology,
}

impl ServerPluginConfig {
    pub fn new(server_config: ServerConfig, protocol: Protocol, topology: Topology) -> Self {
        Self {
            server_config,
            protocol,
            topology,
        }
    }
}

/// Which Bevy world owns naia's server state.
pub enum Topology {
    /// Naia owns the full server, including connection accept/reject APIs.
    Standalone(DriveShape),
    /// An upstream service proxies connections; naia owns only the world server.
    WorldProxied(DriveShape),
    /// The consumer owns the simulation world and installs/drives pipeline handles.
    SimIntegration(SimIntegrationConfig),
}

/// How naia drives the server engine.
pub enum DriveShape {
    Resident,
    Pipelined(crate::plugin_full::PipelineConfig),
}

/// Configuration for [`Topology::SimIntegration`].
#[derive(Default)]
pub struct SimIntegrationConfig {
    pub change_detection_schedule: Option<InternedScheduleLabel>,
    pub skip_host_sync_change_tracking: bool,
}

impl SimIntegrationConfig {
    pub fn with_schedule<S: ScheduleLabel>(mut self, schedule: S) -> Self {
        self.change_detection_schedule = Some(schedule.intern());
        self
    }

    pub fn skip_host_sync(mut self, skip: bool) -> Self {
        self.skip_host_sync_change_tracking = skip;
        self
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
    server_mode: ServerMode,
    /// `Topology::SimIntegration` mode.
    /// When `true`, the plugin registers shared types + message types +
    /// `ComponentEventRegistry` + system sets + `world_to_host_sync`, but
    /// SKIPS constructing the `ServerImpl` resource. The caller installs
    /// `CoordHandle`/`RecvHandle`/`SendHandle` separately via
    /// `naia_server::pipeline_actors::spawn_server_handles` and drives the
    /// recv/apply/send phases through `apply_receive_output_pipeline` +
    /// `apply_recv_to_world`.
    state_external: bool,
    /// Optional override for the schedule under which per-Replicate
    /// `on_component_added` / `on_component_removed` change-tracking
    /// systems are registered. When `None`, the default `Update` schedule
    /// is used. Set through [`SimIntegrationConfig::with_schedule`] or
    /// [`crate::PipelineConfig::with_schedule`] for custom schedules like
    /// cyberlith Sim's `SimMain`.
    change_detection_schedule: Option<InternedScheduleLabel>,
    /// MISSION_USER_ONLY_SEES_SIM Phase D: when `true`,
    /// `Topology::WorldProxied(DriveShape::Pipelined(_))` builds the pipeline,
    /// stores it inside the unified
    /// `ServerImpl::WorldOnly(WorldServer)` resource (§2f), installs the
    /// `ServerEntityConverter` + `EventReceiverRes` Sim resources, and registers
    /// the panic-propagation (and, when opted in, park-window bracket) systems in
    /// `change_detection_schedule` (or `Update`).
    full_pipelining: bool,
    /// When `true`, `build` SKIPS registering the per-`Replicate` host-sync
    /// change-tracking systems (`WorldData::add_systems[_to_schedule]`). Set
    /// by config. MISSION_OVERLAP_FRONTIER T2 — lets an app
    /// whose world hosts no replicated entities (cyberlith base game cell's
    /// main world) drop ~2 no-op change-tracking systems per component type.
    skip_host_sync_change_tracking: bool,
    /// MISSION_PIPELINE_API_BOUNDARY G8 (§2l) — when `true`, `install_full_pipelining`
    /// registers the adapter-driven park-window bracket in the `ReceivePackets`
    /// / `SendPackets` sets (see [`crate::PipelineConfig::drive_bracket_in_update`]).
    /// Set only by `Topology::WorldProxied(DriveShape::Pipelined(_))`.
    drive_bracket_in_update: bool,
}

impl Plugin {
    /// Creates the plugin from an explicit topology and drive shape.
    pub fn new(config: ServerPluginConfig) -> Self {
        let ServerPluginConfig {
            server_config,
            protocol,
            topology,
        } = config;

        match topology {
            Topology::Standalone(DriveShape::Resident) => Self::new_impl(
                server_config,
                protocol,
                false,
                false,
                false,
                None,
                false,
                ServerMode::Resident,
                false,
                false,
            ),
            Topology::Standalone(DriveShape::Pipelined(cfg)) => Self::new_impl(
                server_config,
                protocol,
                false,
                false,
                false,
                cfg.change_detection_schedule,
                false,
                ServerMode::Pipelined,
                cfg.skip_main_world_host_sync,
                false,
            ),
            Topology::WorldProxied(DriveShape::Resident) => Self::new_impl(
                server_config,
                protocol,
                true,
                false,
                false,
                None,
                false,
                ServerMode::Resident,
                false,
                false,
            ),
            Topology::WorldProxied(DriveShape::Pipelined(cfg)) => Self::new_impl(
                server_config,
                protocol,
                true,
                true,
                true,
                cfg.change_detection_schedule,
                true,
                ServerMode::Pipelined,
                cfg.skip_main_world_host_sync,
                cfg.drive_bracket_in_update,
            ),
            Topology::SimIntegration(cfg) => Self::new_impl(
                server_config,
                protocol,
                true,
                true,
                true,
                cfg.change_detection_schedule,
                false,
                ServerMode::Resident,
                cfg.skip_host_sync_change_tracking,
                false,
            ),
        }
    }

    // The single private funnel every public `new_*` constructor delegates
    // to; one parameter per knob is the point.
    #[allow(clippy::too_many_arguments)]
    fn new_impl(
        server_config: ServerConfig,
        protocol: Protocol,
        world_only: bool,
        pipeline: bool,
        state_external: bool,
        change_detection_schedule: Option<InternedScheduleLabel>,
        full_pipelining: bool,
        server_mode: ServerMode,
        skip_host_sync_change_tracking: bool,
        drive_bracket_in_update: bool,
    ) -> Self {
        let config = PluginConfig::new(server_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
            world_only,
            pipeline,
            server_mode,
            state_external,
            change_detection_schedule,
            full_pipelining,
            skip_host_sync_change_tracking,
            drive_bracket_in_update,
        }
    }
}

impl PluginType for Plugin {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().deref_mut().take().unwrap();

        // Take server-event installers before the protocol is consumed by
        // `into()` or `install_full_pipelining`.  Applied after
        // `ComponentEventRegistry` is initialized below.
        let server_event_installers = config.protocol.take_server_event_installers();

        let world_data = config.protocol.take_world_data();
        // T2: skip the per-Replicate host-sync change-tracking systems when the
        // app's world hosts no replicated entities (they would be pure no-op
        // dispatch). The WorldData resource is still inserted; only the
        // on_component_added/removed systems are omitted. `world_to_host_sync`
        // (registered separately) then drains an always-empty event buffer.
        if !self.skip_host_sync_change_tracking {
            if let Some(schedule) = self.change_detection_schedule {
                world_data.add_systems_to_schedule(app, schedule);
                // SharedPlugin registers the entity-level host-sync trackers
                // (`on_host_owned_added` feeding HostOwnedMap, `on_despawn`
                // emitting HostSyncEvent::Despawn) in `Update` — which a Sim
                // SubApp never runs. Mirror them into the change-detection
                // schedule alongside the per-component trackers, with the
                // same map-written-before-read ordering constraint.
                app.add_systems(
                    schedule,
                    on_host_owned_added.in_set(HostSyncOwnedAddedTracking),
                )
                .add_systems(schedule, on_despawn.in_set(HostSyncChangeTracking))
                .configure_sets(
                    schedule,
                    HostSyncOwnedAddedTracking.before(HostSyncChangeTracking),
                );
            } else {
                world_data.add_systems(app);
            }
        }
        app.insert_resource(world_data);

        // Phase B.7: types_and_sets_only skips constructing the
        // ServerImpl resource. The caller installs CoordHandle /
        // RecvHandle / SendHandle via `spawn_server_handles` and drives
        // recv/apply/send through the `apply_*_pipeline` entry points.
        //
        // Phase D (pipelined): we DO construct the three
        // handles here (state_external=true AND full_pipelining=true)
        // and stash them inside the unified `ServerImpl::WorldOnly(
        // WorldServer)` resource for `listen()` to adopt. See
        // `plugin_full.rs`.
        let server_impl = if self.state_external {
            if self.full_pipelining {
                crate::plugin_full::install_full_pipelining(
                    app,
                    config.server_config,
                    config.protocol,
                    self.change_detection_schedule,
                    self.drive_bracket_in_update,
                );
            } else {
                let _ = config.server_config;
            }
            None
        } else if !self.world_only {
            let server = Server::<Entity>::new(
                self.server_mode,
                config.server_config,
                config.protocol.into(),
            );
            Some(ServerImpl::full(server))
        } else {
            let protocol: NaiaProtocol = config.protocol.into();
            let server = match self.server_mode {
                ServerMode::Resident => WorldServer::<Entity>::new(config.server_config, protocol),
                ServerMode::Pipelined => {
                    WorldServer::<Entity>::new_pipelined(config.server_config, protocol)
                }
            };
            Some(ServerImpl::world_only(server))
        };

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<Singleton>::new())
            // RESOURCES //
            .init_resource::<ComponentEventRegistry>();

        // Apply server component-event installers captured during protocol
        // build.  `ComponentEventRegistry` is now present in the world, so
        // `add_component_events::<C>()` can safely look it up.
        for installer in server_event_installers {
            installer(app);
        }

        if let Some(impl_) = server_impl {
            app.insert_resource(impl_);
        }
        app
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
            .configure_sets(Update, WorldToHostSync.before(SendPackets));

        // world_to_host_sync uses ResMut<ServerImpl> via resource_scope.
        // In state_external mode (Phase B.7b) ServerImpl doesn't exist;
        // the caller (cyberlith's GameCell::update) drives an equivalent
        // pipeline-flavored host-sync explicitly via its own helper.
        if !self.state_external {
            app.add_systems(Update, world_to_host_sync.in_set(WorldToHostSync));
        }

        // Recv/translate/send systems are driven by the pipeline coordinator
        // in pipeline mode — skip registering them in `Update`.
        if !self.pipeline {
            app.add_systems(Update, receive_packets.in_set(ReceivePackets))
                .add_systems(Update, process_packets.in_set(ProcessPackets))
                .add_systems(Update, translate_world_events.in_set(TranslateWorldEvents))
                .add_systems(Update, translate_tick_events.in_set(TranslateTickEvents))
                .add_systems(Startup, send_packets_init)
                .add_systems(Update, send_packets.in_set(SendPackets));
        }
    }
}
