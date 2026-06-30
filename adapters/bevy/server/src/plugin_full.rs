//! `Plugin::pipelined` — Plugin variant that internally owns the Recv + Send
//! worker threads and the pipeline runtime.
//!
//! # MISSION_PIPELINE_API_BOUNDARY §2f — PipelinedServer resource removal
//!
//! The pipeline is now stored in the unified `ServerImpl::WorldOnly(WorldServer)`
//! resource (via [`naia_server::WorldServer::from_pipelined`]) so the standard
//! [`crate::Server`] `SystemParam` works in pipelined mode. The separate
//! `PipelinedServer` Bevy resource and the per-handle `Res` wrappers
//! (`SnapshotSenderRes` / `SnapshotReceiverRes` / `SendHandleRes` /
//! `RecvHandleRes`) plus the `PluginInternalState` lifecycle resource are gone:
//! the worker runtime + snapshot channel + recv subscriber are created INTERNALLY
//! by [`naia_server::pipeline_actors::PipelinedWorldServer::start_workers`], and
//! lifecycle (listen / start / park / unpark / panic-propagation) is driven via
//! the static [`crate::Server::pipeline_listen`] / `pipeline_start` /
//! `pipeline_park` / `pipeline_unpark` / `pipeline_propagate_panics` helpers.
//!
//! # API contract
//!
//! Resources installed on the Sim app:
//!
//! - [`crate::ServerEntityConverter`] — for `EntityProperty::set` work in Sim systems.
//! - [`EventReceiverRes`] — the bevy fan-out target for lifecycle/tick/message events.
//! - `ServerImpl` (crate-internal) — holds `WorldServer::from_pipelined(pipeline)`.
//!
//! The plugin is **bound + spawned** by the consumer (or a test) calling
//! [`crate::Server::pipeline_listen`] (bind the socket) followed by
//! [`crate::Server::pipeline_start`] (the separate spawn step). This matches the
//! `Server::listen` pattern: the plugin can't know the consumer's transport
//! choice at `build` time.
//!
//! # Hard rule — do NOT add a process-global mutex
//!
//! The per-thread override + main-side park (D6) is the correct pattern.

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
    resource::Resource,
    world::World,
};

use naia_bevy_shared::{Protocol as BevyProtocol, ReceivePackets, SendPackets};
use naia_server::{
    pipeline_actors::{spawn_server_handles, EventReceiver},
    shared::Protocol as NaiaProtocol,
    ServerConfig, WorldServer,
};

use crate::apply_receive_output_pipeline_with_event_receiver_split;
use crate::server::ServerImpl;
use crate::server_entity_converter::ServerEntityConverter;

// ─── PipelineConfig ────────────────────────────────────────────────────────

/// Tunables for [`Plugin::pipelined`].
#[derive(Default)]
pub struct PipelineConfig {
    /// Schedule under which per-`Replicate` `on_component_added` /
    /// `on_component_removed` change-tracking systems are registered.
    /// When `None`, defaults to bevy's `Update` (matches
    /// `Plugin::sim_integration`).
    pub change_detection_schedule: Option<InternedScheduleLabel>,
    /// When `true`, SKIP registering the per-`Replicate` host-sync
    /// change-tracking systems (`on_component_added`/`on_component_removed`,
    /// the `HostSyncChangeTracking` set) on this app's world.
    ///
    /// Default `false` (register — backward compatible). Set `true` only when
    /// this app's world hosts **zero** replicated entities, so the ~2 systems
    /// per component type are pure no-op dispatch. cyberlith's base game cell
    /// sets this (Sim owns all gameplay; the main world replicates nothing);
    /// the level-editor cell leaves it `false` (its main world DOES host
    /// delegated tile/spawn-point entities). MISSION_OVERLAP_FRONTIER T2.
    pub skip_main_world_host_sync: bool,
    /// MISSION_PIPELINE_API_BOUNDARY G8 (§2l) — when `true`, the adapter itself
    /// drives the per-tick park-window bracket from the existing
    /// `ReceivePackets` / `SendPackets` system sets:
    /// - `ReceivePackets` ⇒ [`PipelinedWorldServer::receive`] (parks internally).
    /// - `SendPackets`    ⇒ [`PipelinedWorldServer::send`] (unparks internally).
    /// The consumer's own systems sit between the two sets via plain
    /// `add_systems(Update, …)`, running with the workers parked — turnkey
    /// pipelining with zero new consumer-facing concepts.
    ///
    /// Default `false` (the adapter registers only the panic-propagation system;
    /// the consumer hand-rolls its own park window). Single-world only.
    ///
    /// [`PipelinedWorldServer::receive`]: naia_server::pipeline_actors::PipelinedWorldServer::receive
    /// [`PipelinedWorldServer::send`]: naia_server::pipeline_actors::PipelinedWorldServer::send
    pub drive_bracket_in_update: bool,
}

impl PipelineConfig {
    /// Set the change-detection schedule label.
    pub fn with_schedule<S: ScheduleLabel>(mut self, schedule: S) -> Self {
        self.change_detection_schedule = Some(schedule.intern());
        self
    }

    /// Skip registering host-sync change-tracking (see field docs).
    pub fn skip_host_sync(mut self, skip: bool) -> Self {
        self.skip_main_world_host_sync = skip;
        self
    }

    /// Have the adapter drive the park-window bracket from the `ReceivePackets`
    /// / `SendPackets` system sets (see [`Self::drive_bracket_in_update`]).
    pub fn drive_in_update(mut self, drive: bool) -> Self {
        self.drive_bracket_in_update = drive;
        self
    }
}

// ─── Resources ──────────────────────────────────────────────────────────────

/// Bevy `Resource` wrapper for the Sim-side [`EventReceiver`]`<Entity>` clone.
/// The bracket's recv fan-out routes lifecycle/tick/message events into this
/// receiver (in addition to the bevy `Messages<…>` buffers); Sim systems pull
/// events via `.0.drain_*()`.
#[derive(Resource, Clone)]
pub struct EventReceiverRes(pub EventReceiver<Entity>);

// ─── install_full_pipelining ───────────────────────────────────────────────

/// Called by `Plugin::build` when `full_pipelining=true`. Builds the pipeline,
/// stores it inside the `ServerImpl::WorldOnly(WorldServer)` resource, installs
/// the `ServerEntityConverter` + `EventReceiverRes` Sim resources, and (when the
/// consumer opted into `drive_bracket_in_update`) registers the adapter-driven
/// park-window bracket in the `ReceivePackets` / `SendPackets` sets. A
/// panic-propagation system is always registered in the change-detection
/// schedule (or `Update`).
pub(crate) fn install_full_pipelining(
    app: &mut App,
    server_config: ServerConfig,
    protocol: BevyProtocol,
    change_detection_schedule: Option<InternedScheduleLabel>,
    drive_bracket_in_update: bool,
) {
    let naia_proto: NaiaProtocol = protocol.into();
    let pipeline = spawn_server_handles::<Entity, _>(server_config, naia_proto);

    let sim_converter = ServerEntityConverter::from_coord(pipeline.coord());
    let sim_event_receiver = EventReceiver::<Entity>::new();

    // The pipeline lives inside the unified `WorldServer` enum so the standard
    // `Server` SystemParam works in pipelined mode. The worker runtime + snapshot
    // channel + recv subscriber are created internally by `start_workers`.
    app.insert_resource(ServerImpl::world_only(WorldServer::from_pipelined(pipeline)));
    app.insert_resource(sim_converter);
    app.insert_resource(EventReceiverRes(sim_event_receiver));

    // Panic propagation runs every tick in the consumer's change-detection
    // schedule (or Update). No-op until the worker runtime is Running.
    if let Some(label) = change_detection_schedule {
        app.add_systems(label, pipelined_propagate_panics);
    } else {
        app.add_systems(bevy_app::Update, pipelined_propagate_panics);
    }

    // G8 §2l — adapter-driven park-window bracket. `ReceivePackets` runs the
    // core `receive` (parks internally); the consumer's own systems run between
    // the sets with the workers parked; `SendPackets` runs the core `send`
    // (unparks internally). Single-world only.
    if drive_bracket_in_update {
        app.add_systems(bevy_app::Update, pipelined_receive.in_set(ReceivePackets))
            .add_systems(bevy_app::Update, pipelined_send.in_set(SendPackets));
    }
}

// ─── G8 adapter-driven park-window bracket systems ─────────────────────────

/// `ReceivePackets` (pipelined, opt-in G8 §2l): drive the core
/// `PipelinedWorldServer::receive` against the single entity world, then fan the
/// returned `ReceiveOutput`s into the bevy `Messages<…>` buffers +
/// `EventReceiver`. `receive` parks the workers internally for the duration of
/// the call. No-op until the worker runtime is `Running` (so recv-draining
/// before `listen()`/`start()` cannot panic in `Server::receive_packet`).
fn pipelined_receive(world: &mut World) {
    use naia_bevy_shared::WorldProxyMut;

    // Clone the event fan-out target before the `&mut World` resource_scope.
    let sim_receiver = world.resource::<EventReceiverRes>().0.clone();

    world.resource_scope(|world, mut server: bevy_ecs::world::Mut<ServerImpl>| {
        let Some(ws) = server.as_world_server_mut() else {
            return;
        };
        let Some(ps) = ws.as_pipelined_mut() else {
            return;
        };
        if !ps.is_running() {
            return;
        }
        // Core parks internally, drains the recv worker's output channel + a
        // synchronous straggler, and applies every output to the entity world.
        let outputs = ps.receive(&mut world.proxy_mut());
        // The coord is back in the pipeline now; borrow it for the bevy-specific
        // event fan-out (core routes no events itself, §2h H3).
        let coord = ps.coord();
        for output in outputs {
            if output.is_empty() {
                continue;
            }
            apply_receive_output_pipeline_with_event_receiver_split(
                world,
                None,
                coord,
                &sim_receiver,
                output,
            );
        }
    });
}

/// `SendPackets` (pipelined, opt-in G8 §2l): drive the core
/// `PipelinedWorldServer::send`. `send` is dual-shape (inline oracle transmit in
/// deterministic builds, publish-to-worker in production) and unparks the workers
/// internally. No-op until the runtime is `Running`, mirroring
/// [`pipelined_receive`].
fn pipelined_send(world: &mut World) {
    use naia_bevy_shared::WorldProxy;

    world.resource_scope(|world, mut server: bevy_ecs::world::Mut<ServerImpl>| {
        let Some(ws) = server.as_world_server_mut() else {
            return;
        };
        let Some(ps) = ws.as_pipelined_mut() else {
            return;
        };
        if !ps.is_running() {
            return;
        }
        ps.send(&world.proxy());
    });
}

/// Bevy system that surfaces any worker panic onto the main thread. Installed in
/// the change-detection schedule (or `Update`).
fn pipelined_propagate_panics(world: &mut World) {
    crate::Server::pipeline_propagate_panics(world);
}
