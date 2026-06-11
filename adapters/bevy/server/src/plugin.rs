use parking_lot::Mutex;
use std::ops::DerefMut;

use bevy_app::{App, Plugin as PluginType, Startup, Update};
use bevy_ecs::{
    entity::Entity,
    prelude::ApplyDeferred,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
};

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
    /// `types_and_sets_only` mode (Phase B.7 of MISSION_SIM_OWNS_WORLD).
    /// When `true`, the plugin registers shared types + message types +
    /// `ComponentEventRegistry` + system sets + `world_to_host_sync`, but
    /// SKIPS constructing the `ServerImpl` resource. The caller installs
    /// `SimHandle`/`RecvHandle`/`SendHandle` separately via
    /// `naia_server::pipeline_actors::spawn_server_handles` and drives the
    /// recv/apply/send phases through `apply_receive_output_pipeline` +
    /// `apply_recv_to_world`.
    state_external: bool,
    /// Optional override for the schedule under which per-Replicate
    /// `on_component_added` / `on_component_removed` change-tracking
    /// systems are registered. When `None`, the default `Update` schedule
    /// is used (backward compatible with all existing callers — namako,
    /// the existing cyberlith integration). Set via
    /// [`Plugin::sim_integration_with_schedule`] to register under a
    /// custom schedule like cyberlith Sim's `SimMain`.
    change_detection_schedule: Option<InternedScheduleLabel>,
    /// MISSION_USER_ONLY_SEES_SIM Phase D: when `true`,
    /// [`Plugin::sim_integration_full`] also constructs the three
    /// pipeline handles, installs them + the C.6-prep facade
    /// resources (`SimConverter`, `SimEventReceiver`, `SnapshotSender`,
    /// `SnapshotReceiver`, `SimHandleRes`, `SendHandleRes`,
    /// `PluginInternalState`), and registers the main-side
    /// `drain_recv_worker_output` + `propagate_worker_panics` systems
    /// in `change_detection_schedule` (or `Update`).
    full_pipelining: bool,
    /// When `true`, `build` SKIPS registering the per-`Replicate` host-sync
    /// change-tracking systems (`WorldData::add_systems[_to_schedule]`). Set
    /// only by `sim_integration_full` from `PluginSimConfig`; all other
    /// constructors pass `false`. MISSION_OVERLAP_FRONTIER T2 — lets an app
    /// whose world hosts no replicated entities (cyberlith base game cell's
    /// main world) drop ~2 no-op change-tracking systems per component type.
    skip_host_sync_change_tracking: bool,
}

impl Plugin {
    /// Creates the plugin with the given server configuration and protocol.
    pub fn new(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, false, false, false, None, false)
    }

    /// MISSION_USER_ONLY_SEES_SIM Phase D — full-pipelining variant
    /// that internally spawns Recv + Send worker threads.
    ///
    /// See `plugin_full.rs` for the API contract + lifecycle / park
    /// / panic surface exposed via [`crate::PluginInternalState`].
    pub fn sim_integration_full(
        server_config: ServerConfig,
        protocol: Protocol,
        cfg: crate::plugin_full::PluginSimConfig,
    ) -> Self {
        let mut plugin = Self::new_impl(
            server_config,
            protocol,
            true,
            true,
            true,
            cfg.change_detection_schedule,
            true,
        );
        plugin.skip_host_sync_change_tracking = cfg.skip_main_world_host_sync;
        plugin
    }

    /// World-only variant. Skips full `Server` setup (auth, accept_connection,
    /// etc.) and registers a `WorldServer` instead. Used by services that
    /// proxy connections (e.g., the game cell behind a session server).
    pub fn world_only(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, false, false, None, false)
    }

    /// Pipeline + world-only variant (Phase 4 capacity uplift). Builds the
    /// same `WorldServer` as `world_only`, but skips the recv/translate/send
    /// `Update` systems — those phases are driven explicitly by the
    /// pipeline coordinator via `RecvHandle`/`SendHandle`. Naia's host-sync
    /// systems (`HostSyncOwnedAddedTracking`, `HostSyncChangeTracking`,
    /// `WorldToHostSync`) remain in `Update`.
    pub fn world_only_pipeline(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, true, false, None, false)
    }

    /// Phase B.7 variant: installs shared types + system sets +
    /// `world_to_host_sync` but does NOT construct a `ServerImpl`. Used
    /// by the SubApp pipeline coordinator (cyberlith) which installs
    /// `SimHandle`/`RecvHandle`/`SendHandle` via `spawn_server_handles`
    /// and drives recv/apply/send via the `apply_*_pipeline` family.
    ///
    /// `server_config` is unused by this variant (no `ServerImpl` is
    /// built) but kept in the signature for symmetry with the other
    /// constructors; callers may pass `ServerConfig::default()`.
    pub fn types_and_sets_only(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, true, true, None, false)
    }

    /// Iris 2 (MISSION_IRIS_2 / SPEC_IRIS_2_NAIA.md §1.4) — pipelined
    /// Sim-side variant.
    ///
    /// Functionally identical to [`Plugin::types_and_sets_only`]:
    /// installs shared types + system sets + per-`Replicate`
    /// `on_component_added` / `on_component_removed` / `on_despawn`
    /// change-tracking systems via `WorldData::add_systems`, but skips
    /// `world_to_host_sync` (no `ServerImpl` in pipeline mode).
    ///
    /// What's new vs. `types_and_sets_only` is the **documented
    /// intended use**: this constructor signals that the host bevy
    /// world being plugged in is cyberlith's Sim SubApp world (the
    /// gameplay source of truth), NOT the Send-mirror world that
    /// `types_and_sets_only` historically targeted.
    ///
    /// Cyberlith Sim drives the host-sync drain step via the new
    /// [`crate::drain_host_sync_into_pipeline`] helper (called from a
    /// Sim main-schedule system) — this replaces the
    /// `world_to_host_sync` body byte-for-byte but routes through the
    /// three pipeline handles instead of `ResMut<ServerImpl>`.
    ///
    /// Spawn-side semantics are unchanged from the non-pipelined
    /// plugin: cyberlith Sim entities marked with `enable_replication`
    /// (or `commands.spawn((...))` followed by `commands.enable_replication`)
    /// get the `HostOwned` marker, which triggers per-component
    /// `on_component_added` → `HostSyncEvent::Insert` → drain →
    /// `insert_component_worldless` → diff-handler attach. Subsequent
    /// `Mut<R>::deref_mut()` mutations fire the
    /// `PropertyMutate` callback → `GlobalDirtyBitset` mark — exactly
    /// the same chain as today's Send-mirror flow.
    pub fn sim_integration(server_config: ServerConfig, protocol: Protocol) -> Self {
        Self::new_impl(server_config, protocol, true, true, true, None, false)
    }

    /// Variant of [`Plugin::sim_integration`] that registers all per-Replicate
    /// `on_component_added` / `on_component_removed` change-tracking systems
    /// in `schedule` instead of the canonical `Update`.
    ///
    /// Cyberlith's Sim SubApp drives its gameplay logic in a non-`Update`
    /// schedule (`SimMain`). For the Sim-side host-sync flow to fire
    /// correctly, the per-component change-detection systems must also
    /// run in that schedule — `Update` is never invoked on the Sim
    /// SubApp. This constructor threads the schedule label through to
    /// [`crate::naia_bevy_shared::WorldData::add_systems_to_schedule`].
    pub fn sim_integration_with_schedule<S: ScheduleLabel>(
        server_config: ServerConfig,
        protocol: Protocol,
        schedule: S,
    ) -> Self {
        Self::new_impl(
            server_config,
            protocol,
            true,
            true,
            true,
            Some(schedule.intern()),
            false,
        )
    }

    fn new_impl(
        server_config: ServerConfig,
        protocol: Protocol,
        world_only: bool,
        pipeline: bool,
        state_external: bool,
        change_detection_schedule: Option<InternedScheduleLabel>,
        full_pipelining: bool,
    ) -> Self {
        let config = PluginConfig::new(server_config, protocol);
        Self {
            config: Mutex::new(Some(config)),
            world_only,
            pipeline,
            state_external,
            change_detection_schedule,
            full_pipelining,
            // Default: register host-sync change-tracking. Only
            // `sim_integration_full` overrides this from `PluginSimConfig`.
            skip_host_sync_change_tracking: false,
        }
    }
}

impl PluginType for Plugin {
    fn build(&self, app: &mut App) {
        let mut config = self.config.lock().deref_mut().take().unwrap();

        let world_data = config.protocol.take_world_data();
        // T2: skip the per-Replicate host-sync change-tracking systems when the
        // app's world hosts no replicated entities (they would be pure no-op
        // dispatch). The WorldData resource is still inserted; only the
        // on_component_added/removed systems are omitted. `world_to_host_sync`
        // (registered separately) then drains an always-empty event buffer.
        if !self.skip_host_sync_change_tracking {
            if let Some(schedule) = self.change_detection_schedule {
                world_data.add_systems_to_schedule(app, schedule);
            } else {
                world_data.add_systems(app);
            }
        }
        app.insert_resource(world_data);

        // Phase B.7: types_and_sets_only skips constructing the
        // ServerImpl resource. The caller installs SimHandle /
        // RecvHandle / SendHandle via `spawn_server_handles` and drives
        // recv/apply/send through the `apply_*_pipeline` entry points.
        //
        // Phase D (sim_integration_full): we DO construct the three
        // handles here (state_external=true AND full_pipelining=true)
        // and stash them in `PluginInternalState` for `listen()` to
        // adopt. See `plugin_full.rs`.
        let server_impl = if self.state_external {
            if self.full_pipelining {
                crate::plugin_full::install_full_pipelining(
                    app,
                    config.server_config,
                    config.protocol,
                    self.change_detection_schedule,
                );
            } else {
                let _ = config.server_config;
            }
            None
        } else if !self.world_only {
            let server = Server::<Entity>::new(config.server_config, config.protocol.into());
            Some(ServerImpl::full(server))
        } else {
            let protocol: NaiaProtocol = config.protocol.into();
            let server = WorldServer::<Entity>::new(config.server_config, protocol);
            Some(ServerImpl::world_only(server))
        };

        app
            // SHARED PLUGIN //
            .add_plugins(SharedPlugin::<Singleton>::new())
            // RESOURCES //
            .init_resource::<ComponentEventRegistry>();
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
