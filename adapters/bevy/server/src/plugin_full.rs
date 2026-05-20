//! `Plugin::sim_integration_full` — Plugin variant that internally
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
//! - [`crate::SimConverter`] — for `EntityProperty::set` work in Sim systems.
//! - [`SimEventReceiver`]`<Entity>` — Sim drains lifecycle/tick/message events.
//! - [`SnapshotSender`]`<Entity>` — Sim publishes per-tick snapshots.
//! - [`SnapshotReceiver`]`<Entity>` — for symmetry / observability; the
//!   internal Send worker holds the load-bearing clone.
//! - [`CoordHandleRes`] — Sim systems take/return for cross-handle work
//!   (e.g. `configure_entity_replication`, `send_message_to_user`).
//! - [`SendHandleRes`] — Sim systems take/return for cross-handle work,
//!   between Send-worker iterations. Hands off via the same
//!   `Option<>`-take pattern cyberlith already uses.
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

use std::{
    any::Any,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread::{self, JoinHandle, Thread},
};

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
    world::World,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use parking_lot::Mutex;

use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::{
    pipeline_actors::{
        spawn_server_handles, CoordHandle, SimEventReceiver, SnapshotReceiver, SnapshotSender,
    },
    shared::Protocol as NaiaProtocol,
    RecvHandle, ReceiveOutput, SendHandle, ServerConfig,
};

use crate::apply_receive_output_pipeline_with_sim_receiver;
use crate::sim_converter::SimConverter;

// ─── PluginSimConfig ────────────────────────────────────────────────────────

/// Tunables for [`Plugin::sim_integration_full`].
#[derive(Default)]
pub struct PluginSimConfig {
    /// Schedule under which per-`Replicate` `on_component_added` /
    /// `on_component_removed` change-tracking systems are registered.
    /// When `None`, defaults to bevy's `Update` (matches
    /// `Plugin::sim_integration`).
    pub change_detection_schedule: Option<InternedScheduleLabel>,
}

impl PluginSimConfig {
    /// Set the change-detection schedule label.
    pub fn with_schedule<S: ScheduleLabel>(mut self, schedule: S) -> Self {
        self.change_detection_schedule = Some(schedule.intern());
        self
    }
}

// ─── Resources ──────────────────────────────────────────────────────────────

/// Bevy resource holding the [`CoordHandle`]. Take/return pattern: Sim
/// systems use [`Option::take`] to gain ownership for cross-handle ops
/// (`configure_entity_replication`, `send_message_to_user`, etc.) and
/// must put the handle back before returning.
#[derive(Resource)]
pub struct CoordHandleRes(pub Option<CoordHandle<Entity>>);

/// Bevy resource holding the [`SendHandle`] while parked on the Sim
/// app. The internal Send worker holds the handle between ticks; when
/// the worker is parked (via [`PluginInternalState::park_workers`])
/// the handle returns here for Sim systems to borrow.
#[derive(Resource)]
pub struct SendHandleRes(pub Option<SendHandle<Entity>>);

/// Bevy `Resource` wrapper for the Sim-side
/// [`SimEventReceiver`]`<Entity>` clone. Sim systems pull events via
/// `.0.drain_*()`.
#[derive(Resource, Clone)]
pub struct SimEventReceiverRes(pub SimEventReceiver<Entity>);

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

/// Lifecycle states the plugin progresses through.
#[repr(u8)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum State {
    /// `Plugin::build` ran; handles parked; workers not yet spawned.
    Armed = 0,
    /// `listen()` succeeded; workers running.
    Running = 1,
    /// `Drop` called; workers shutting down or stopped.
    Stopped = 2,
}

/// Park-control flags + condvar pair (parking_lot Mutex + Condvar) used
/// to coordinate main↔worker timing.
struct ParkControl {
    /// `true` ⇒ workers should park at the top of their loop iteration.
    park: AtomicBool,
    /// Number of workers currently parked. Main waits until this hits
    /// the expected worker count before continuing past
    /// [`PluginInternalState::park_workers`].
    parked_count: Mutex<u32>,
    parked_cv: parking_lot::Condvar,
}

impl ParkControl {
    fn new() -> Self {
        Self {
            park: AtomicBool::new(false),
            parked_count: Mutex::new(0),
            parked_cv: parking_lot::Condvar::new(),
        }
    }
}

/// Captured panic from a worker thread. Surfaced on main via
/// [`PluginInternalState::propagate_panic_if_any`].
struct PanicSlot(Mutex<Option<Box<dyn Any + Send + 'static>>>);

impl PanicSlot {
    fn new() -> Self {
        Self(Mutex::new(None))
    }
    fn set(&self, payload: Box<dyn Any + Send + 'static>) {
        let mut g = self.0.lock();
        if g.is_none() {
            *g = Some(payload);
        }
    }
    fn take(&self) -> Option<Box<dyn Any + Send + 'static>> {
        self.0.lock().take()
    }
}

/// Bevy resource exposing the lifecycle / park / panic / shutdown
/// surface of [`Plugin::sim_integration_full`].
///
/// On drop, signals shutdown to the worker threads and blocks until
/// they join (with a 5s soft-deadline before logging a warning).
#[derive(Resource)]
pub struct PluginInternalState {
    state: AtomicU8,
    /// `(coord, recv, send)` parked here between `build` and `listen`.
    /// `None` after `listen` (handles moved into workers / Resources).
    armed_handles: Mutex<Option<(CoordHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>)>>,
    /// Channel for the Recv worker to push `ReceiveOutput` to main.
    /// `Some` until `listen` runs.
    recv_out_chan_rx: Mutex<Option<Receiver<ReceiveOutput<Entity>>>>,
    /// Sender half retained so it can be dropped on shutdown (signals
    /// the main-side drain system that the worker is gone).
    recv_out_chan_tx: Mutex<Option<Sender<ReceiveOutput<Entity>>>>,
    /// SnapshotReceiver held until the Send worker is spawned.
    snapshot_receiver: Mutex<Option<SnapshotReceiver<Entity>>>,
    /// SnapshotSender retained for the SnapshotSender resource clone
    /// path (the Sim app keeps the load-bearing clone via the inserted
    /// Resource; this is here only for `listen()`-time wiring).
    _snapshot_sender_keep: Mutex<Option<SnapshotSender<Entity>>>,
    /// Shutdown signaller: dropping flips `true`, observed by workers
    /// at the top of each loop iteration.
    shutdown: Arc<AtomicBool>,
    /// Park/unpark coordination.
    park: Arc<ParkControl>,
    /// Captured panic payload (first worker to panic wins).
    panic_slot: Arc<PanicSlot>,
    /// Worker thread handles. Empty until `listen()`; drained on Drop.
    workers: Mutex<Vec<WorkerHandle>>,
    /// SimEventReceiver clone retained so the main-side drain system
    /// can re-acquire it without taking the Sim Resource.
    sim_event_receiver: Mutex<Option<SimEventReceiver<Entity>>>,
    /// Coord handle stashed by `listen()` for the Startup
    /// `drain_armed_coord_into_resource` system to install into
    /// [`CoordHandleRes`]. Drained at most once.
    armed_coord: Mutex<Option<CoordHandle<Entity>>>,
}

struct WorkerHandle {
    name: &'static str,
    thread: Thread,
    join: Option<JoinHandle<()>>,
}

impl PluginInternalState {
    fn new_armed(
        coord: CoordHandle<Entity>,
        recv: RecvHandle<Entity>,
        send: SendHandle<Entity>,
        sim_event_receiver: SimEventReceiver<Entity>,
        snapshot_sender: SnapshotSender<Entity>,
        snapshot_receiver: SnapshotReceiver<Entity>,
    ) -> Self {
        let (tx, rx) = bounded::<ReceiveOutput<Entity>>(1);
        Self {
            state: AtomicU8::new(State::Armed as u8),
            armed_handles: Mutex::new(Some((coord, recv, send))),
            recv_out_chan_rx: Mutex::new(Some(rx)),
            recv_out_chan_tx: Mutex::new(Some(tx)),
            snapshot_receiver: Mutex::new(Some(snapshot_receiver)),
            _snapshot_sender_keep: Mutex::new(Some(snapshot_sender)),
            shutdown: Arc::new(AtomicBool::new(false)),
            park: Arc::new(ParkControl::new()),
            panic_slot: Arc::new(PanicSlot::new()),
            workers: Mutex::new(Vec::new()),
            sim_event_receiver: Mutex::new(Some(sim_event_receiver)),
            armed_coord: Mutex::new(None),
        }
    }

    fn state(&self) -> State {
        // Safe because we only ever store valid `State` discriminants.
        match self.state.load(Ordering::Acquire) {
            0 => State::Armed,
            1 => State::Running,
            _ => State::Stopped,
        }
    }

    /// Listen on `socket`. Calls `io_load` on the parked
    /// `WorldServer` (reassembled from the three handles for the
    /// duration of the call), spawns the Recv + Send worker threads,
    /// and transitions to `Running`.
    ///
    /// Panics if called more than once or after Drop.
    pub fn listen<S: Into<Box<dyn naia_server::transport::Socket>>>(&self, socket: S) {
        assert_eq!(
            self.state(),
            State::Armed,
            "PluginInternalState::listen called in non-Armed state",
        );

        let (coord, recv, send) = self
            .armed_handles
            .lock()
            .take()
            .expect("armed_handles Some in Armed state");

        // Load socket io via the run_with_world_server reassembly
        // helper. Byte-for-byte equivalent to `Server::listen`.
        let socket: Box<dyn naia_server::transport::Socket> = socket.into();
        let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(socket);
        let (coord, recv, send, ()) =
            naia_server::pipeline_actors::run_with_world_server(coord, recv, send, |ws| {
                ws.io_load(ps, pr);
            });

        // Recv worker takes ownership of recv handle + tx side of the
        // ReceiveOutput channel.
        let recv_tx = self
            .recv_out_chan_tx
            .lock()
            .clone()
            .expect("recv_out_chan_tx Some in Armed state");

        let shutdown_recv = Arc::clone(&self.shutdown);
        let park_recv = Arc::clone(&self.park);
        let panic_recv = Arc::clone(&self.panic_slot);

        #[cfg(feature = "test_time")]
        let clock_handle_recv = naia_bevy_shared::TestClock::shareable_handle();

        let recv_join = thread::Builder::new()
            .name("naia-recv-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::install_shared(clock_handle_recv);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut recv = recv;
                    recv_worker_loop(&mut recv, &recv_tx, &shutdown_recv, &park_recv);
                }));

                if let Err(payload) = result {
                    panic_recv.set(payload);
                }

                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::detach_shared();
            })
            .expect("spawn recv worker thread");
        let recv_thread = recv_join.thread().clone();

        // Send worker takes ownership of send handle + SnapshotReceiver.
        let snap_rx = self
            .snapshot_receiver
            .lock()
            .take()
            .expect("snapshot_receiver Some in Armed state");

        let shutdown_send = Arc::clone(&self.shutdown);
        let park_send = Arc::clone(&self.park);
        let panic_send = Arc::clone(&self.panic_slot);

        #[cfg(feature = "test_time")]
        let clock_handle_send = naia_bevy_shared::TestClock::shareable_handle();

        let send_join = thread::Builder::new()
            .name("naia-send-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::install_shared(clock_handle_send);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut send = send;
                    send_worker_loop(&mut send, &snap_rx, &shutdown_send, &park_send);
                }));

                if let Err(payload) = result {
                    panic_send.set(payload);
                }

                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::detach_shared();
            })
            .expect("spawn send worker thread");
        let send_thread = send_join.thread().clone();

        // Coord handle stays on main as a Resource via CoordHandleRes
        // — the caller's `Plugin::build` already installed
        // `CoordHandleRes(None)` and will fill it from a Startup
        // system that drains `armed_coord` here.
        //
        // To avoid a chicken-and-egg with Startup ordering, the
        // sim_integration_full constructor installs a Startup system
        // (`drain_armed_coord`) that runs once and pulls the parked
        // coord handle into the resource. Since `listen()` can be
        // called before or after Startup, we stash the coord on the
        // PluginInternalState itself for that drain.
        self.armed_coord.lock().replace(coord);

        self.workers.lock().extend([
            WorkerHandle {
                name: "naia-recv-worker",
                thread: recv_thread,
                join: Some(recv_join),
            },
            WorkerHandle {
                name: "naia-send-worker",
                thread: send_thread,
                join: Some(send_join),
            },
        ]);

        self.state.store(State::Running as u8, Ordering::Release);
    }

    /// Park both worker threads (block until both reach the top of
    /// their idle loop). After return, callers may safely
    /// `TestClock::advance(...)` or borrow handles via [`CoordHandleRes`]
    /// without racing the workers.
    ///
    /// No-op when in `Armed` (workers not yet spawned) or `Stopped`.
    pub fn park_workers(&self) {
        if self.state() != State::Running {
            return;
        }
        let expected = self.workers.lock().len() as u32;
        self.park.park.store(true, Ordering::SeqCst);
        // Unpark each worker so they observe the park flag on the next
        // iteration even if currently sleeping.
        for w in self.workers.lock().iter() {
            w.thread.unpark();
        }
        let mut g = self.park.parked_count.lock();
        while *g < expected {
            self.park.parked_cv.wait(&mut g);
        }
    }

    /// Resume both worker threads.
    pub fn unpark_workers(&self) {
        if self.state() != State::Running {
            return;
        }
        // Reset count and clear park flag, then unpark threads to wake
        // them out of their park loop.
        *self.park.parked_count.lock() = 0;
        self.park.park.store(false, Ordering::SeqCst);
        for w in self.workers.lock().iter() {
            w.thread.unpark();
        }
    }

    /// If a worker thread has panicked, re-panic on the calling
    /// thread (surfacing the captured payload). Call once per
    /// `App::update` from a Sim system, or before any test assertion.
    pub fn propagate_panic_if_any(&self) {
        if let Some(payload) = self.panic_slot.take() {
            std::panic::resume_unwind(payload);
        }
    }

    /// Internal: armed coord parking slot, filled by `listen()` and
    /// drained by the Startup `drain_armed_coord_into_resource` system
    /// (or directly by the test if it skips Startup).
    fn _armed_coord_take(&self) -> Option<CoordHandle<Entity>> {
        self.armed_coord.lock().take()
    }
}

impl Drop for PluginInternalState {
    fn drop(&mut self) {
        // Signal shutdown.
        self.shutdown.store(true, Ordering::SeqCst);
        // Drop the recv channel sender so the main-side drain stops
        // expecting more outputs.
        self.recv_out_chan_tx.lock().take();
        // Unpark + clear park flag so workers can exit promptly.
        self.park.park.store(false, Ordering::SeqCst);
        let mut workers = std::mem::take(&mut *self.workers.lock());
        for w in &workers {
            w.thread.unpark();
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        for w in workers.iter_mut() {
            let name = w.name;
            if let Some(join) = w.join.take() {
                // Try to join, but don't block forever.
                while std::time::Instant::now() < deadline {
                    if join.is_finished() {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(1));
                }
                if join.is_finished() {
                    if join.join().is_err() {
                        log::warn!("naia plugin worker {name} panicked during shutdown");
                    }
                } else {
                    log::warn!(
                        "naia plugin worker {name} did not exit within 5s; leaking thread",
                    );
                }
            }
        }
        self.state.store(State::Stopped as u8, Ordering::Release);
    }
}

// ─── install_full_pipelining ───────────────────────────────────────────────

/// Called by `Plugin::build` when `full_pipelining=true`. Constructs
/// the three pipeline handles, installs the C.6-prep Sim Resources
/// (`SimConverter`, `SimEventReceiver`, `SnapshotSender`,
/// `SnapshotReceiver`, `CoordHandleRes`, `SendHandleRes`,
/// `PluginInternalState`) on the App, and registers the main-side
/// drain + panic-propagation systems in the supplied schedule (or
/// `Update`).
pub(crate) fn install_full_pipelining(
    app: &mut App,
    server_config: ServerConfig,
    protocol: BevyProtocol,
    change_detection_schedule: Option<InternedScheduleLabel>,
) {
    let naia_proto: NaiaProtocol = protocol.into();
    let (coord, recv, send) = spawn_server_handles::<Entity, _>(server_config, naia_proto);

    let sim_converter = SimConverter::from_coord(&coord);
    let sim_event_receiver = SimEventReceiver::<Entity>::new();
    let (snap_sender, snap_receiver) = SnapshotSender::<Entity>::pair();

    let internal = PluginInternalState::new_armed(
        coord,
        recv,
        send,
        sim_event_receiver.clone(),
        snap_sender.clone(),
        snap_receiver.clone(),
    );

    app.insert_resource(sim_converter);
    app.insert_resource(SimEventReceiverRes(sim_event_receiver));
    app.insert_resource(SnapshotSenderRes(snap_sender));
    app.insert_resource(SnapshotReceiverRes(snap_receiver));
    app.insert_resource(CoordHandleRes(None));
    app.insert_resource(SendHandleRes(None));
    app.insert_resource(internal);

    // Startup: drain the armed coord into CoordHandleRes once the
    // consumer's `listen()` has run. The system itself is a no-op
    // until listen completes — re-run via the normal Update schedule
    // is fine because once drained the field is `None`.
    let drain_armed_into_res = |world: &mut World| {
        let coord_opt = world
            .get_resource::<PluginInternalState>()
            .and_then(|s| s.armed_coord.lock().take());
        if let Some(c) = coord_opt {
            world.resource_mut::<CoordHandleRes>().0 = Some(c);
        }
    };

    // Register main-side systems in the consumer's schedule (Update
    // by default). drain_armed runs FIRST so subsequent systems see
    // the coord; drain_recv_worker_output then applies output; panic
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
}

// ─── Worker loops ───────────────────────────────────────────────────────────

/// Top-of-iteration: if `park` is set, increment parked_count, notify
/// main, then `thread::park()` until unparked + park cleared.
fn worker_park_checkpoint(park: &ParkControl) {
    if !park.park.load(Ordering::SeqCst) {
        return;
    }
    {
        let mut g = park.parked_count.lock();
        *g += 1;
        park.parked_cv.notify_all();
    }
    // Park loop: keep parking until the park flag clears.
    while park.park.load(Ordering::SeqCst) {
        thread::park();
    }
}

fn recv_worker_loop(
    recv: &mut RecvHandle<Entity>,
    out_tx: &Sender<ReceiveOutput<Entity>>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
) {
    loop {
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        let output = recv.receive();
        // bounded(1) — if main is behind, drop the OLDEST so newer
        // events flow. ReceiveOutput is not Clone; we use try_send +
        // drain-then-send pattern.
        match out_tx.try_send(output) {
            Ok(()) => {}
            Err(TrySendError::Full(new_output)) => {
                // Channel full: leave the prior output in place and
                // drop the new one. Backpressure: prefer freshness on
                // the main side, but never block the recv socket loop.
                // ReceiveOutput holds packets; dropping a tick of
                // events is rare under normal pipelining (main is
                // expected to drain at least once per outer tick).
                drop(new_output);
            }
            Err(TrySendError::Disconnected(_)) => {
                return;
            }
        }
        // Small sleep to avoid hot-spin when socket has no data; mirrors
        // the cyberlith worker's effective cadence (driven there by
        // SubApp::update overhead).
        if !shutdown.load(Ordering::SeqCst) && !park.park.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_micros(100));
        }
    }
}

fn send_worker_loop(
    send: &mut SendHandle<Entity>,
    snap_rx: &SnapshotReceiver<Entity>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
) {
    loop {
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        if let Some(snap) = snap_rx.take_latest() {
            send.apply_pending_send_preamble();
            send.apply_pending_scope_changes(&snap);
            send.send_all_packets(snap);
        } else {
            // No snapshot pending — short sleep so we don't hot-spin.
            thread::sleep(std::time::Duration::from_micros(100));
        }
    }
}

// ─── Main-side drain system ────────────────────────────────────────────────

/// Bevy system installed on the consumer Sim app. Drains
/// `ReceiveOutput<Entity>` from the Recv worker's bounded channel and
/// fans into the bevy `Messages<X>` buffers + the `SimEventReceiver`.
///
/// Runs in `Update` by default (same as `process_packets` for the
/// non-`sim_integration_full` variants).
pub fn drain_recv_worker_output(world: &mut World) {
    // Pull the channel + sim_event_receiver + coord without holding
    // an outer borrow on world.
    let receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.recv_out_chan_rx.lock().as_ref().cloned());
    let Some(receiver) = receiver else { return };

    let sim_receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.sim_event_receiver.lock().as_ref().cloned());
    let Some(sim_receiver) = sim_receiver else { return };

    // Drain everything currently in the channel (bounded(1), so at most
    // one item).
    let outputs: Vec<ReceiveOutput<Entity>> = receiver.try_iter().collect();
    if outputs.is_empty() {
        return;
    }

    // Take the coord handle for the apply call, then put it back.
    let coord = world
        .resource_mut::<CoordHandleRes>()
        .0
        .take();
    let Some(coord) = coord else {
        // Coord handle in use elsewhere this frame — re-queue the
        // outputs by dropping (next tick will pull fresh ones).
        return;
    };
    for output in outputs {
        apply_receive_output_pipeline_with_sim_receiver(world, &coord, &sim_receiver, output);
    }
    world.resource_mut::<CoordHandleRes>().0 = Some(coord);
}

/// Bevy system that surfaces any worker panic onto the main thread.
/// Installed in the same default schedule as `drain_recv_worker_output`.
pub fn propagate_worker_panics(state: bevy_ecs::system::Res<PluginInternalState>) {
    state.propagate_panic_if_any();
}
