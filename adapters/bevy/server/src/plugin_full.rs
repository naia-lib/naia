//! `Plugin::pipelined` — Plugin variant that internally
//! owns the Recv + Send worker threads + the three pipeline handles
//! (`CoordHandle` / `RecvHandle` / `SendHandle`).
//!
//! # MISSION_USER_ONLY_SEES_SIM Phase D
//!
//! The post-MISSION_SIM_OWNS_WORLD architecture has cyberlith's user
//! code only ever touching the Sim app — Recv and Send are pure naia
//! plumbing. Today cyberlith still hosts `recv_app.rs` + `send_app.rs`
//! + `pipelined_recv.rs` as SubApp wrappers + a worker thread to drive
//! the existing facades. Phase D moves the worker-thread + naia
//! per-tick loop entirely inside naia. After Phase E lands in cyberlith,
//! the consumer installs only this plugin and never sees the naia
//! Recv/Send SubApps or worker-thread infrastructure.
//!
//! # API contract
//!
//! Resources installed on the Sim app:
//!
//! - [`crate::ServerEntityConverter`] — for `EntityProperty::set` work in Sim systems.
//! - [`EventReceiver`]`<Entity>` — Sim drains lifecycle/tick/message events.
//! - [`SnapshotSender`]`<Entity>` — Sim publishes per-tick snapshots.
//! - [`SnapshotReceiver`]`<Entity>` — for symmetry / observability; the
//!   internal Send worker holds the load-bearing clone.
//! - [`PipelinedServer`] — holds the unified pipeline; Sim systems access the
//!   `CoordHandle` via `PipelinedServer::0.as_mut().map(|p| p.coord_mut())`.
//! - [`SendHandleRes`] — Sim systems take/return for cross-handle work,
//!   between Send-worker iterations. Hands off via the same
//!   `Option<>`-take pattern cyberlith already uses.
//! - [`RecvHandleRes`] — Sim systems take/return the recv worker's
//!   `RecvHandle` while workers are parked, to drain naia's per-user
//!   tick-buffered `PlayerCommands` via
//!   [`naia_server::RecvHandle::receive_tick_buffer_messages`] (Phase
//!   D.3a). Wraps the worker's shared park-window slot; see the type
//!   doc for the race-free borrow contract.
//! - [`PluginInternalState`] — lifecycle / park / panic control surface.
//!
//! The Plugin is **armed but not running** until the consumer calls
//! [`PluginInternalState::listen`] with a socket. This matches the
//! existing `Server::listen` / `WorldServer::io_load` pattern: the
//! Plugin can't know the consumer's transport choice at `build` time.
//!
//! # D6 TestClock integration
//!
//! When the `test_time` feature is active, `Plugin::build` captures a
//! shareable [`naia_bevy_shared::TestClock`] handle on main. Each
//! spawned worker installs that shared handle as the first line of its
//! closure, matching the cyberlith `pipelined_recv::spawn` pattern at
//! [[project-d6-testclock-findings]]. The Phase G discipline of parking
//! workers before driving `TestClock::advance(...)` is exposed via
//! [`PluginInternalState::park_workers`].
//!
//! # Hard rule — do NOT add a process-global mutex
//!
//! The F2 attempt at process-global synchronization deadlocked. D6's
//! per-thread override + main-side park is the correct pattern.

use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
    world::World,
};
use parking_lot::Mutex;

use naia_bevy_shared::{Protocol as BevyProtocol, ReceivePackets, SendPackets};
use naia_server::{
    pipeline_actors::{
        spawn_server_handles, EventReceiver, PipelineRuntime, PipelinedServer as CorePipelinedServer,
        RuntimeState, RuntimeTimingHooks, SnapshotReceiver, SnapshotSender,
    },
    shared::Protocol as NaiaProtocol,
    ReceiveOutput, RecvHandle, SendHandle, ServerConfig,
};

use crate::apply_receive_output_pipeline_with_event_receiver_split;
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
    /// - `ReceivePackets` ⇒ `park_workers()` + the single-world recv-drain
    ///   ([`drain_recv_impl`]).
    /// - `SendPackets` ⇒ [`PipelinedServer::send`] (dual send-shape) +
    ///   `unpark_workers()`.
    /// The consumer's own systems sit between the two sets via plain
    /// `add_systems(Update, …)`, running with the workers parked and the handles
    /// round-tripping through their slots — turnkey pipelining with zero new
    /// consumer-facing concepts.
    ///
    /// Default `false` (the adapter registers only the no-op drain/panic systems;
    /// the consumer hand-rolls its own park window, e.g. cyberlith's
    /// `open_park_window` until the G10 cutover). Opt-in is what lets G8 land
    /// non-breaking: the existing pipelined adapter tests drive park/unpark
    /// manually and rely on the adapter NOT auto-driving a window in `Update`.
    /// Single-world only (`entity_world = None`); a Sim-SubApp split stays a
    /// consumer choice (§2f).
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

/// Bevy resource holding the [`PipelinedServer`].
///
/// `None` until the armed pipeline is drained into this resource (via the
/// `drain_armed_into_res` Startup system or
/// [`PluginInternalState::drain_armed_pipeline_into_resource`]).
/// `Some` once `listen()` has completed and the drain has run.
///
/// # Park-window borrow contract
///
/// Workers must be parked (via [`PluginInternalState::park_workers`]) before
/// any code calls [`PipelinedServer::take_handles`] or [`PipelinedServer::tick`] on
/// the pipeline inside this resource. Callers must unpark via
/// [`PluginInternalState::unpark_workers`] once the pipeline is restored.
#[derive(Resource)]
pub struct PipelinedServer(pub Option<CorePipelinedServer<Entity>>);

/// Bevy resource exposing the send worker's [`SendHandle`] for the
/// park-window borrow contract.
///
/// Like [`RecvHandleRes`] (and for the same reason — the internal Send
/// worker owns the handle for its socket loop), this wraps the **same**
/// `Arc<Mutex<…>>` park-window slot the worker borrows from. While the
/// workers are parked the handle is guaranteed to be in the slot, so a
/// Sim system can take it for cross-half work that needs the send half
/// (e.g. `apply_recv_to_world`'s `process_recv_packets` decode step,
/// `send_message_to_user`), then return it before
/// [`PluginInternalState::unpark_workers`].
///
/// (D.3a promoted this from the prior `Option`-only stub to the shared
/// slot so the recv-side tick-buffer drain — which depends on
/// `process_recv_packets` having decoded the buffers, a send-half op —
/// is reachable on main.)
///
/// The wrapped `Arc` is the **same** slot as `PipelinedServer`'s pipeline holds
/// (`sim_pipeline.send_slot()`) — callers that already have `PipelinedServer`
/// can access it via [`PipelinedServer::take_handles`] / [`PipelinedServer::tick`] instead.
#[derive(Resource, Clone)]
pub struct SendHandleRes(pub Arc<Mutex<Option<SendHandle<Entity>>>>);

/// Bevy resource exposing the recv worker's [`RecvHandle`] for the
/// park-window borrow contract (MISSION_USER_ONLY_SEES_SIM Phase D.3a).
///
/// The wrapped `Arc` is the **same** slot as `PipelinedServer`'s pipeline holds
/// (`sim_pipeline.recv_slot()`).
///
/// See [`SendHandleRes`] for the full borrow-contract explanation.
#[derive(Resource, Clone)]
pub struct RecvHandleRes(pub Arc<Mutex<Option<RecvHandle<Entity>>>>);

/// Bevy `Resource` wrapper for the Sim-side
/// [`EventReceiver`]`<Entity>` clone. Sim systems pull events via
/// `.0.drain_*()`.
#[derive(Resource, Clone)]
pub struct EventReceiverRes(pub EventReceiver<Entity>);

/// Bevy `Resource` wrapper for the Sim-side
/// [`SnapshotSender`]`<Entity>` clone. Sim publishes per-tick snapshots
/// via `.0.send(snap)`.
#[derive(Resource, Clone)]
pub struct SnapshotSenderRes(pub SnapshotSender<Entity>);

/// Bevy `Resource` wrapper for the [`SnapshotReceiver`]`<Entity>` clone
/// (the load-bearing copy lives in the Send worker; this is here for
/// observability / advanced consumers).
#[derive(Resource, Clone)]
pub struct SnapshotReceiverRes(pub SnapshotReceiver<Entity>);

// ─── PluginInternalState ───────────────────────────────────────────────────

/// Bevy resource: the bevy-coupled lifecycle / park / panic surface of
/// [`Plugin::pipelined`]. The worker-thread runtime itself now lives in
/// naia-server core ([`PipelineRuntime`], MISSION_PIPELINE_API_BOUNDARY G7-3);
/// this resource holds only the bevy-only wiring and **delegates** lifecycle
/// (`listen`/`park`/`unpark`/panic-propagation/shutdown) to it.
#[derive(Resource)]
pub struct PluginInternalState {
    /// [`PipelinedServer`] parked here between `build` and the drain into
    /// [`PipelinedServer`]. `listen()` reassembles `WorldServer` for `io_load`
    /// by calling `with_world_server` on this pipeline (which temporarily moves
    /// handles into a `WorldServer` and returns them). After `listen()`, the
    /// pipeline is back here until `drain_armed_pipeline_into_resource` moves it
    /// into `PipelinedServer`.
    armed_pipeline: Mutex<Option<CorePipelinedServer<Entity>>>,
    /// SnapshotSender retained for the SnapshotSender resource clone
    /// path (the Sim app keeps the load-bearing clone via the inserted
    /// Resource; this is here only for `listen()`-time wiring).
    _snapshot_sender_keep: Mutex<Option<SnapshotSender<Entity>>>,
    /// EventReceiver clone retained so the main-side drain system
    /// can re-acquire it without taking the Sim Resource.
    sim_event_receiver: Mutex<Option<EventReceiver<Entity>>>,
    /// Park-window slot for the recv worker's [`RecvHandle`] — the SAME `Arc`
    /// the core runtime + the pipeline hold, wrapped by [`RecvHandleRes`] so a
    /// parked-window Sim system can take/return the handle.
    recv_slot: Arc<Mutex<Option<RecvHandle<Entity>>>>,
    /// Park-window slot for the send worker's [`SendHandle`] — symmetric.
    /// Wrapped by [`SendHandleRes`].
    send_slot: Arc<Mutex<Option<SendHandle<Entity>>>>,
    /// The framework-agnostic worker runtime (threads + park barrier + recv
    /// channel + snapshot-lag wiring + Armed→Running→Stopped lifecycle). Its
    /// `Drop` signals shutdown and joins the workers (5s soft-deadline).
    runtime: PipelineRuntime<Entity>,
}

impl PluginInternalState {
    fn new_armed(
        sim_pipeline: CorePipelinedServer<Entity>,
        sim_event_receiver: EventReceiver<Entity>,
        snapshot_sender: SnapshotSender<Entity>,
        snapshot_receiver: SnapshotReceiver<Entity>,
    ) -> Self {
        // The shared slot Arcs from the pipeline: the core runtime is given the
        // SAME Arcs at construction so workers and the consumer's park-window
        // borrow share one underlying `Mutex<Option<…>>`.
        let recv_slot = sim_pipeline.recv_slot();
        let send_slot = sim_pipeline.send_slot();

        // Wire the bench-only `pipeline_timing` aggregator into the core runtime
        // via fn-pointer hooks. Zero overhead + zero coupling when the feature
        // is off (the core runtime takes `Option<fn(u64)>` per stage).
        #[allow(unused_mut)]
        let mut timing = RuntimeTimingHooks::default();
        #[cfg(feature = "pipeline_timing")]
        {
            timing.record_recv = Some(crate::pipeline_timing::record_recv);
            timing.record_send = Some(crate::pipeline_timing::record_send);
            timing.record_barrier = Some(crate::pipeline_timing::record_barrier);
        }

        let runtime = PipelineRuntime::new_armed(
            Arc::clone(&recv_slot),
            Arc::clone(&send_slot),
            snapshot_receiver,
            timing,
        );

        Self {
            armed_pipeline: Mutex::new(Some(sim_pipeline)),
            _snapshot_sender_keep: Mutex::new(Some(snapshot_sender)),
            sim_event_receiver: Mutex::new(Some(sim_event_receiver)),
            recv_slot,
            send_slot,
            runtime,
        }
    }

    /// `Arc` clone of the recv worker's park-window slot, wrapping the same
    /// `Arc` held by [`PipelinedServer`]'s pipeline.
    fn recv_handle_res(&self) -> RecvHandleRes {
        RecvHandleRes(Arc::clone(&self.recv_slot))
    }

    /// `Arc` clone of the send worker's park-window slot, wrapping the same
    /// `Arc` held by [`PipelinedServer`]'s pipeline.
    fn send_handle_res(&self) -> SendHandleRes {
        SendHandleRes(Arc::clone(&self.send_slot))
    }

    /// MISSION_PIPELINE_API_BOUNDARY G8b — wire the recv worker's output channel
    /// into the armed [`PipelinedServer`]'s `recv_subscriber` so its `receive`
    /// drains the worker (the mirror of `set_send_publisher`, which is wired
    /// pre-construction because its channel is created in `install_full_pipelining`).
    ///
    /// Done post-construction because the recv-output channel is created INSIDE
    /// [`PipelineRuntime::new_armed`] (the runtime owns it), so its [`Receiver`]
    /// does not exist until `internal` is built. No-op if the runtime has already
    /// shut down (`recv_out_receiver()` is `None`) or the armed pipeline has been
    /// drained.
    #[cfg(not(feature = "deterministic"))]
    fn wire_recv_subscriber_into_armed(&self) {
        if let Some(rx) = self.runtime.recv_out_receiver() {
            if let Some(p) = self.armed_pipeline.lock().as_mut() {
                p.set_recv_subscriber(rx);
            }
        }
    }

    /// Test/dev hook: request that workers panic on their next loop iteration.
    /// Delegates to the core runtime.
    #[cfg(any(test, feature = "test_time"))]
    pub fn request_worker_panic_for_test(&self) {
        self.runtime.request_worker_panic_for_test();
    }

    /// Listen on `socket`. Binds the socket on the parked `WorldServer`
    /// (reassembled from the three handles for the duration of the call), then
    /// asks the core [`PipelineRuntime`] to spawn the Recv + Send worker threads
    /// and transition `Armed → Running`.
    ///
    /// Panics if called more than once or after Drop (the core runtime asserts
    /// the `Armed` precondition in `spawn_workers`).
    pub fn listen<S: Into<Box<dyn naia_server::transport::Socket>>>(&self, socket: S) {
        let mut sim_pipeline = self
            .armed_pipeline
            .lock()
            .take()
            .expect("armed_pipeline Some in Armed state");

        // Bind the socket (G2 API): splits the socket into I/O handles and
        // calls io_load on the reassembled WorldServer in one step.
        sim_pipeline.listen(socket);

        // Event-driven readiness (Some for the in-process PacketChannel every
        // cell uses; None for poll-only sockets): the recv worker selects on it
        // instead of polling. The recv handle is back in its slot after
        // `listen()` returns (same Arc as self.recv_slot / the runtime's).
        let recv_readiness = self
            .recv_slot
            .lock()
            .as_ref()
            .expect("recv handle in slot after listen()")
            .readiness();

        // Spawn the worker threads + flip Armed→Running (core runtime owns the
        // threading, park barrier, clock-sharing, panic capture, and channel).
        self.runtime.spawn_workers(recv_readiness);

        // Stash the pipeline back in armed_pipeline. A Startup system
        // (`drain_armed_pipeline_into_resource`) drains it into
        // PipelinedServer(Some(...)) once the bevy world is ready. Since
        // listen() can be called before or after Startup, this intermediate
        // stash avoids a chicken-and-egg ordering issue.
        self.armed_pipeline.lock().replace(sim_pipeline);
    }

    /// Park both worker threads (block until both reach the top of their idle
    /// loop). After return, callers may safely `TestClock::advance(...)` or
    /// borrow handles via [`PipelinedServer`] / [`RecvHandleRes`] /
    /// [`SendHandleRes`] without racing the workers. No-op when `Armed` /
    /// `Stopped`. Panic/exit-safe (see [`PipelineRuntime::park_workers`]).
    /// Delegates to the core runtime.
    pub fn park_workers(&self) {
        self.runtime.park_workers();
    }

    /// Resume both worker threads. Synchronous — see
    /// [`PipelineRuntime::unpark_workers`]. Delegates to the core runtime.
    pub fn unpark_workers(&self) {
        self.runtime.unpark_workers();
    }

    /// If a worker thread has panicked, re-panic on the calling thread.
    /// Call once per `App::update` from a Sim system, or before any test
    /// assertion. Delegates to the core runtime.
    pub fn propagate_panic_if_any(&self) {
        self.runtime.propagate_panic_if_any();
    }

    ///
    /// Take the armed [`PipelinedServer`] from the parking slot.
    ///
    /// Filled at construction time; consumed by the Startup
    /// `drain_armed_into_res` system (or by consumers that need to avoid a
    /// combined `&World` / `&mut World` borrow aliasing issue, e.g. cyberlith
    /// `init.rs`). Returns `None` once the slot has been drained (idempotent).
    ///
    /// Pattern that avoids the aliasing issue:
    /// ```ignore
    /// if let Some(pipeline) = world.resource::<PluginInternalState>().take_armed_pipeline() {
    ///     world.resource_mut::<PipelinedServer>().0 = Some(pipeline);
    /// }
    /// ```
    pub fn take_armed_pipeline(&self) -> Option<CorePipelinedServer<Entity>> {
        self.armed_pipeline.lock().take()
    }

    /// Drain the armed [`PipelinedServer`] into [`PipelinedServer`] on `world`.
    ///
    /// Called by consumers (e.g. cyberlith `init.rs`) that drive Startup
    /// manually before running `Update` — the `drain_armed_into_res` closure
    /// registered in `Update` would only fire after Startup, but they need the
    /// pipeline available during Startup. Safe to call multiple times (no-op
    /// once the slot is empty).
    ///
    /// Note: `self` must NOT be borrowed from `world` when this is called
    /// (to avoid aliasing `&PluginInternalState` and `&mut World`). Use
    /// [`Self::take_armed_pipeline`] + direct insert if you need to call
    /// this while `PluginInternalState` is still borrowed from `world`.
    pub fn drain_armed_pipeline_into_resource(&self, world: &mut bevy_ecs::world::World) {
        if let Some(p) = self.armed_pipeline.lock().take() {
            world.resource_mut::<PipelinedServer>().0 = Some(p);
        }
    }

    /// Obtain a [`naia_server::pipeline_actors::SendStateView`] from the
    /// pre-`listen()` armed pipeline's [`CoordHandle`].
    ///
    /// Called by consumers (e.g. cyberlith E.6c `init.rs`) that need to
    /// pass `send_state_view` to `install_sim_plugins` before `listen()`
    /// is invoked. Reads from `armed_pipeline` (the pipeline lives there
    /// until drained into [`PipelinedServer`] after `listen()`). Panics
    /// if the armed pipeline has been drained.
    pub fn armed_send_state_view(&self) -> naia_server::pipeline_actors::SendStateView<Entity> {
        let guard = self.armed_pipeline.lock();
        guard
            .as_ref()
            .expect("armed_send_state_view called after pipeline drained — too late")
            .coord()
            .send_state_view()
    }
}

// ─── install_full_pipelining ───────────────────────────────────────────────

/// Called by `Plugin::build` when `full_pipelining=true`. Constructs
/// the three pipeline handles, installs the C.6-prep Sim Resources
/// (`ServerEntityConverter`, `EventReceiver`, `SnapshotSender`,
/// `SnapshotReceiver`, `PipelinedServer`, `SendHandleRes`,
/// `PluginInternalState`) on the App, and registers the main-side
/// drain + panic-propagation systems in the supplied schedule (or
/// `Update`).
pub(crate) fn install_full_pipelining(
    app: &mut App,
    server_config: ServerConfig,
    protocol: BevyProtocol,
    change_detection_schedule: Option<InternedScheduleLabel>,
    drive_bracket_in_update: bool,
) {
    let naia_proto: NaiaProtocol = protocol.into();
    #[cfg_attr(feature = "deterministic", allow(unused_mut))]
    let mut sim_pipeline = spawn_server_handles::<Entity, _>(server_config, naia_proto);

    let sim_converter = ServerEntityConverter::from_coord(sim_pipeline.coord());
    let sim_event_receiver = EventReceiver::<Entity>::new();
    let (snap_sender, snap_receiver) = SnapshotSender::<Entity>::pair();

    // G8 §2l Decision 1 — in worker-driven production builds, when the adapter
    // drives the bracket, point `PipelinedServer::send` at the SAME snapshot
    // channel the runtime's send worker drains, so `send` publishes the frozen
    // one-tick-lag job instead of transmitting inline. Deterministic builds (and
    // any non-driven build) leave it unset ⇒ the inline oracle shape.
    //
    // The adapter keys this off its own `deterministic` feature (which forwards
    // 1:1 to `naia-server/deterministic`, the same feature `naia-server`'s
    // `build.rs` reads to set `workers_active = not(deterministic)` core-side) —
    // the adapter no longer carries a `build.rs` (G7-3), so it cannot read the
    // `workers_active` cfg directly. `not(deterministic)` is exactly the
    // production/bench build where the send worker actively transmits.
    #[cfg(not(feature = "deterministic"))]
    if drive_bracket_in_update {
        sim_pipeline.set_send_publisher(snap_sender.clone());
    }

    let internal = PluginInternalState::new_armed(
        sim_pipeline,
        sim_event_receiver.clone(),
        snap_sender.clone(),
        snap_receiver.clone(),
    );

    // G8b — mirror of the `set_send_publisher` wiring above, but post-construction:
    // point the armed pipeline's `recv_subscriber` at the runtime's recv-output
    // channel so `PipelinedServer::receive` drains the recv worker. The channel is
    // born inside `PipelineRuntime::new_armed` (called by `new_armed`), so the
    // `Receiver` only exists now. Same `not(deterministic)` + opt-in gate as send.
    #[cfg(not(feature = "deterministic"))]
    if drive_bracket_in_update {
        internal.wire_recv_subscriber_into_armed();
    }

    let recv_handle_res = internal.recv_handle_res();
    let send_handle_res = internal.send_handle_res();

    app.insert_resource(sim_converter);
    app.insert_resource(EventReceiverRes(sim_event_receiver));
    app.insert_resource(SnapshotSenderRes(snap_sender));
    app.insert_resource(SnapshotReceiverRes(snap_receiver));
    app.insert_resource(PipelinedServer(None));
    app.insert_resource(send_handle_res);
    app.insert_resource(recv_handle_res);
    app.insert_resource(internal);

    // Runs every Update tick until the pipeline drains (no-op once empty).
    // Drains the armed PipelinedServer into PipelinedServer once listen() has run.
    let drain_armed_into_res = |world: &mut World| {
        let pipeline_opt = world
            .get_resource::<PluginInternalState>()
            .and_then(|s| s.take_armed_pipeline());
        if let Some(p) = pipeline_opt {
            world.resource_mut::<PipelinedServer>().0 = Some(p);
        }
    };

    // Register main-side systems in the consumer's schedule (Update
    // by default). drain_armed runs FIRST so subsequent systems see
    // the CoordHandle; drain_recv_worker_output then applies output; panic
    // propagation runs last.
    if let Some(label) = change_detection_schedule {
        app.add_systems(
            label,
            (
                drain_armed_into_res,
                drain_recv_worker_output,
                propagate_worker_panics,
            )
                .chain(),
        );
    } else {
        app.add_systems(
            bevy_app::Update,
            (
                drain_armed_into_res,
                drain_recv_worker_output,
                propagate_worker_panics,
            )
                .chain(),
        );
    }

    // G8 §2l — when the consumer opted in, the adapter drives the park-window
    // bracket from the existing `ReceivePackets` / `SendPackets` system sets
    // (configured in `Update` by `Plugin::build`). `ReceivePackets` parks the
    // workers and runs the single-world recv-drain; `SendPackets` runs the core
    // bracket's `send` (dual-shape) and unparks. The consumer's own systems sit
    // between the two sets (the set chain orders them) with workers parked.
    //
    // `drain_armed_into_res` is chained BEFORE `pipelined_receive` inside
    // `ReceivePackets` so the `PipelinedServer` resource is populated before the
    // bracket runs — both bracket systems no-op (and crucially do NOT park) until
    // then, keeping park/unpark balanced across the listen()/drain transition.
    if drive_bracket_in_update {
        app.add_systems(
            bevy_app::Update,
            (drain_armed_into_res, pipelined_receive)
                .chain()
                .in_set(ReceivePackets),
        )
        .add_systems(bevy_app::Update, pipelined_send.in_set(SendPackets));
    }
}

// ─── G8 adapter-driven park-window bracket systems ─────────────────────────

/// `ReceivePackets` (pipelined, opt-in G8 §2l): open the park window and run the
/// single-world recv-drain. Parks the workers, then drains the recv worker's
/// output channel + a synchronous `recv.receive()` and applies/fans the events
/// (via [`drain_recv_impl`], `entity_world = None`). Leaves the workers parked
/// for the consumer's systems; [`pipelined_send`] closes the window.
///
/// No-op (and does NOT park) until the worker runtime is `Running` (i.e. after
/// `listen()` has spawned the workers), so the window stays balanced with
/// [`pipelined_send`] across the arming transition. Note `drain_armed_into_res`
/// populates the `PipelinedServer` resource even BEFORE `listen()` (so consumers
/// can reach the coord at `Startup`), so the listening signal is the runtime
/// state, not resource presence — recv-draining before `listen()` would panic in
/// `Server::receive_packet`.
fn pipelined_receive(world: &mut World) {
    use naia_bevy_shared::WorldProxyMut;

    // Gate on Running + park the workers (open the window). Release the
    // `&PluginInternalState` borrow before the `&mut World` work below.
    {
        let state = world.resource::<PluginInternalState>();
        if state.runtime.state() != RuntimeState::Running {
            return;
        }
        state.park_workers();
    }

    // Clone the event-fan-out target so the `&PluginInternalState` borrow is
    // dropped before the `resource_scope` below mutably borrows `world`.
    let sim_receiver = world
        .resource::<PluginInternalState>()
        .sim_event_receiver
        .lock()
        .as_ref()
        .cloned();
    let Some(sim_receiver) = sim_receiver else {
        return;
    };

    // G8b: the recv drain now lives in core `PipelinedServer::receive`. Pull the
    // pipeline OUT of `world` (resource_scope) so we can hold `pipeline.coord()`
    // for the bevy event fan-out while the diminished `world` is borrowed `&mut`.
    world.resource_scope(|world, mut ps: bevy_ecs::world::Mut<PipelinedServer>| {
        let Some(pipeline) = ps.0.as_mut() else {
            return;
        };
        // Core drains the recv worker's output channel (wired via
        // `recv_subscriber`) + a synchronous straggler, and applies every output
        // to the single entity world. Single-world only (`entity_world = None`);
        // the dual-world `_split` path stays on `drain_recv_impl` until G10.
        let outputs = pipeline.receive(&mut world.proxy_mut());
        // The coord is back in the pipeline now; borrow it for the bevy-specific
        // event fan-out (core routes no events itself, §2h H3). Fan out per output
        // in FIFO order into the bevy `Messages<…>` buffers + `EventReceiver`.
        let coord = pipeline.coord();
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

/// `SendPackets` (pipelined, opt-in G8 §2l): run the core bracket's `send` and
/// close the park window (unpark). `send` is dual-shape — inline transmit
/// (oracle / deterministic) or publish-to-worker (production), selected by the
/// `send_publisher` wired in [`install_full_pipelining`].
///
/// No-op (and does NOT unpark) until the runtime is `Running`, mirroring
/// [`pipelined_receive`] so park/unpark stay balanced across arming.
fn pipelined_send(world: &mut World) {
    use naia_bevy_shared::WorldProxy;

    if world.resource::<PluginInternalState>().runtime.state() != RuntimeState::Running {
        return;
    }
    world.resource_scope(|world, mut ps: bevy_ecs::world::Mut<PipelinedServer>| {
        if let Some(pipeline) = ps.0.as_mut() {
            pipeline.send(&world.proxy());
        }
    });
    world.resource::<PluginInternalState>().unpark_workers();
}

// ─── Main-side drain system ────────────────────────────────────────────────

/// Core receive drain logic: drains `ReceiveOutput` from the channel
/// and from a synchronous `recv.receive()` call, applies cross-half
/// orchestration, and fans events into the bevy `Messages<X>` buffers
/// + the `EventReceiver`.
///
/// **Requires workers to already be parked** before this is called.
/// The caller is responsible for parking/unparking. Handles are taken
/// from the provided slots and returned before this function returns.
/// Returns `(sim_handle, recv, send)` with the updated handles.
pub fn drain_recv_impl(
    world: &mut World,
    recv_slot: &Arc<Mutex<Option<RecvHandle<Entity>>>>,
    send_slot: &Arc<Mutex<Option<SendHandle<Entity>>>>,
) {
    drain_recv_impl_split(world, None, recv_slot, send_slot)
}

/// Dual-target variant of [`drain_recv_impl`].
///
/// `entity_world` is the world that hosts client-published replicated
/// entities when that world is NOT the coordinator world (e.g. cyberlith's
/// editor cells, whose replicated entities live on the Sim SubApp world).
/// When `Some`, the recv apply targets it for everything entity-scoped:
///
/// - `apply_recv_to_world`'s world proxy (entity spawns/despawns, component
///   insert/update/remove, publish + delegation arms) mutates `entity_world`;
/// - the entity-scoped event fan-out (Spawn/Despawn/Publish/Unpublish
///   Messages, `ClientOwned`/`HostOwned` marker mutations, and the
///   `ComponentEventRegistry` tail) fires into `entity_world` — see
///   [`apply_receive_output_pipeline_with_event_receiver_split`].
///
/// Connection-scoped state stays on `world` (the coordinator): the
/// `PluginInternalState` / handle-slot reads, and the Tick / Connect /
/// Disconnect / Error / Message / Request / Auth event buffers.
///
/// `entity_world: None` is byte-identical to [`drain_recv_impl`].
pub fn drain_recv_impl_split(
    world: &mut World,
    mut entity_world: Option<&mut World>,
    recv_slot: &Arc<Mutex<Option<RecvHandle<Entity>>>>,
    send_slot: &Arc<Mutex<Option<SendHandle<Entity>>>>,
) {
    use naia_bevy_shared::WorldProxyMut;
    use naia_server::pipeline_actors::apply_recv_to_world;

    let receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.runtime.recv_out_receiver());
    let Some(receiver) = receiver else { return };

    let sim_receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.sim_event_receiver.lock().as_ref().cloned());
    let Some(sim_receiver) = sim_receiver else {
        return;
    };

    // Take only the CoordHandle from the pipeline — recv/send are already
    // available via the caller-provided slot Arcs (same underlying Arcs).
    let sim_handle_opt = world
        .resource_mut::<PipelinedServer>()
        .0
        .as_mut()
        .map(|p| p.take_coord());
    let recv_opt = recv_slot.lock().take();
    let send_opt = send_slot.lock().take();

    if sim_handle_opt.is_none() || recv_opt.is_none() || send_opt.is_none() {
        if let Some(c) = sim_handle_opt {
            if let Some(p) = world.resource_mut::<PipelinedServer>().0.as_mut() {
                p.restore_coord(c);
            }
        }
        if let Some(r) = recv_opt {
            *recv_slot.lock() = Some(r);
        }
        if let Some(s) = send_opt {
            *send_slot.lock() = Some(s);
        }
        return;
    }

    let mut sim_handle = sim_handle_opt.unwrap();
    let mut recv = recv_opt.unwrap();
    let mut send = send_opt.unwrap();

    // Drain everything the recv worker has shipped to the channel since the
    // last park window. The channel is unbounded, so this collects ALL outputs
    // produced between drains in FIFO order — none were dropped (see `new_armed`).
    let mut outputs: Vec<ReceiveOutput<Entity>> = receiver.try_iter().collect();

    // Perform a synchronous recv.receive() while workers are parked. This
    // catches any packets that arrived after the recv worker's most recent
    // iteration (e.g., packets delivered by process_time_queues AFTER the
    // last worker sleep started).
    #[cfg(feature = "pipeline_timing")]
    let _t_recv = std::time::Instant::now();
    let fresh_output = recv.receive();
    #[cfg(feature = "pipeline_timing")]
    crate::pipeline_timing::record_recv(_t_recv.elapsed().as_nanos() as u64);
    outputs.push(fresh_output);

    if outputs.iter().all(|o| o.is_empty()) {
        if let Some(p) = world.resource_mut::<PipelinedServer>().0.as_mut() {
            p.restore_coord(sim_handle);
        }
        *recv_slot.lock() = Some(recv);
        *send_slot.lock() = Some(send);
        return;
    }

    // Cross-half receive orchestration per ReceiveOutput. This is the
    // RecvApply cost (`apply_recv_to_world` on main) tracked by pipeline_timing
    // — folded into the consumer's single park window as of
    // MISSION_PIPELINE_BARRIER_ORCH Part A.
    #[cfg(feature = "pipeline_timing")]
    let _t_apply = std::time::Instant::now();
    for mut output in outputs {
        if output.is_empty() {
            continue;
        }
        let server_tick = sim_handle.current_tick();
        // Entity ops mutate the entity world (the coordinator world itself
        // in single-world mode).
        let (c, r, s) = match entity_world.as_deref_mut() {
            Some(ew) => apply_recv_to_world(
                sim_handle,
                recv,
                send,
                &mut ew.proxy_mut(),
                &mut output,
                server_tick,
            ),
            None => apply_recv_to_world(
                sim_handle,
                recv,
                send,
                &mut world.proxy_mut(),
                &mut output,
                server_tick,
            ),
        };
        sim_handle = c;
        recv = r;
        send = s;
        apply_receive_output_pipeline_with_event_receiver_split(
            world,
            entity_world.as_deref_mut(),
            &sim_handle,
            &sim_receiver,
            output,
        );
    }
    #[cfg(feature = "pipeline_timing")]
    crate::pipeline_timing::record_apply(_t_apply.elapsed().as_nanos() as u64);

    // Return handles (no unpark — caller's responsibility).
    if let Some(p) = world.resource_mut::<PipelinedServer>().0.as_mut() {
        p.restore_coord(sim_handle);
    }
    *recv_slot.lock() = Some(recv);
    *send_slot.lock() = Some(send);
}

/// Bevy system installed on the consumer Sim app in the `Update` schedule.
///
/// **No-op on BOTH paths** (deterministic and active workers). The full recv
/// drain — collect the recv worker's bounded output channel, a synchronous
/// `recv.receive()`, and `apply_recv_to_world` cross-half orchestration — runs
/// inside the consumer's per-tick park window via [`drain_recv_impl`] (e.g.
/// cyberlith's `open_park_window`).
///
/// History: under `workers_active` this used to park the workers itself, drain
/// + apply, then unpark — a SECOND park on top of the consumer's park window
/// (two barrier waits per tick). MISSION_PIPELINE_BARRIER_ORCH Part A
/// (2026-05-20) collapsed that double park into the single park window: the
/// active path now drains in `drain_recv_impl` exactly like the (byte-exact-
/// tested) deterministic path always has, so this system has nothing to do.
/// It is kept registered (still chained with `drain_armed_into_res` +
/// `propagate_worker_panics`) to preserve the schedule shape; the per-tick
/// dispatch cost is negligible. The earlier `test_time`-only no-op also avoided
/// a hard-to-reproduce park/unpark token race; that is now moot on every path.
pub fn drain_recv_worker_output(world: &mut World) {
    let _ = world;
}

/// Bevy system that surfaces any worker panic onto the main thread.
/// Installed in the same default schedule as `drain_recv_worker_output`.
pub fn propagate_worker_panics(state: bevy_ecs::system::Res<PluginInternalState>) {
    state.propagate_panic_if_any();
}
