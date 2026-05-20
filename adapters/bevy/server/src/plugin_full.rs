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
#[derive(Resource, Clone)]
pub struct SendHandleRes(pub Arc<Mutex<Option<SendHandle<Entity>>>>);

/// Bevy resource exposing the recv worker's [`RecvHandle`] for the
/// park-window borrow contract (MISSION_USER_ONLY_SEES_SIM Phase D.3a).
///
/// # Why a shared `Arc<Mutex<Option<…>>>` (not a plain `Option` like
/// [`CoordHandleRes`])
///
/// The [`CoordHandle`] lives **only** on main — no worker ever owns it —
/// so a plain `Option` Resource is the honest representation. The
/// [`RecvHandle`] is different: the internal Recv worker owns it for its
/// socket loop. Park-window ownership is therefore *shared* between the
/// worker and main: this Resource wraps the **same** `Arc<Mutex<…>>`
/// slot the worker borrows from per-iteration. A plain `Option` would
/// require an extra main-thread ferry system (slot → Res after park,
/// Res → slot before unpark) that is strictly more error-prone, so we
/// surface the shared slot directly.
///
/// # Borrow / drain contract (race-free)
///
/// 1. A Sim system calls [`PluginInternalState::park_workers`]. This
///    blocks until **both** workers are parked at their loop top. At
///    that point the recv worker has **deposited** its `RecvHandle`
///    into this slot (see [`recv_worker_loop`]).
/// 2. The Sim system takes the handle: `recv_res.0.lock().take()`,
///    calls [`RecvHandle::receive_tick_buffer_messages`] for each tick
///    being processed, then puts it back: `*recv_res.0.lock() = Some(h)`.
/// 3. The Sim system calls [`PluginInternalState::unpark_workers`]. The
///    recv worker re-claims the handle from the slot and resumes.
///
/// The park barrier (`parked_count` Condvar) guarantees the deposit
/// **happens-before** `park_workers()` returns, and the re-claim
/// **happens-after** the worker observes the cleared park flag — which
/// itself is sequenced after `unpark_workers()` writes the slot back.
/// No process-global mutex; the only lock is this per-plugin slot.
#[derive(Resource, Clone)]
pub struct RecvHandleRes(pub Arc<Mutex<Option<RecvHandle<Entity>>>>);

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
    /// D.3a park-window slot for the recv worker's [`RecvHandle`]. The
    /// recv worker deposits its handle here at the park checkpoint and
    /// re-claims it on unpark. The same `Arc` is wrapped by the
    /// [`RecvHandleRes`] Resource so a parked-window Sim system can
    /// take/return the handle. `None` until `listen()` spawns the worker.
    recv_slot: Arc<Mutex<Option<RecvHandle<Entity>>>>,
    /// D.3a park-window slot for the send worker's [`SendHandle`] —
    /// symmetric to `recv_slot`. Wrapped by [`SendHandleRes`]. `None`
    /// until `listen()` spawns the worker.
    send_slot: Arc<Mutex<Option<SendHandle<Entity>>>>,
    /// Test-only: when set, workers panic on their next loop iteration
    /// (after the park checkpoint, before any other work). Used by
    /// the D.5 panic-propagation test.
    #[cfg(any(test, feature = "test_time"))]
    test_panic_request: Arc<AtomicBool>,
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
            recv_slot: Arc::new(Mutex::new(None)),
            send_slot: Arc::new(Mutex::new(None)),
            #[cfg(any(test, feature = "test_time"))]
            test_panic_request: Arc::new(AtomicBool::new(false)),
        }
    }

    /// D.3a — a [`RecvHandleRes`] wrapping the **same** park-window slot
    /// `Arc` the recv worker borrows from. The Resource and the worker
    /// thus share ownership of the `RecvHandle`; the park barrier makes
    /// the take/return race-free (see [`RecvHandleRes`]).
    fn recv_handle_res(&self) -> RecvHandleRes {
        RecvHandleRes(Arc::clone(&self.recv_slot))
    }

    /// D.3a — a [`SendHandleRes`] wrapping the same send park-window slot
    /// `Arc` the send worker borrows from. Symmetric to
    /// [`Self::recv_handle_res`].
    fn send_handle_res(&self) -> SendHandleRes {
        SendHandleRes(Arc::clone(&self.send_slot))
    }

    /// Test/dev hook: request that workers panic on their next loop
    /// iteration. Available under `cfg(test)` or with the `test_time`
    /// feature (already used by all bevy adapter integration tests).
    #[cfg(any(test, feature = "test_time"))]
    pub fn request_worker_panic_for_test(&self) {
        self.test_panic_request.store(true, Ordering::SeqCst);
        // Unpark workers so they observe the request promptly.
        for w in self.workers.lock().iter() {
            w.thread.unpark();
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

        // Recv worker borrows the recv handle from the shared park-window
        // slot per-iteration (D.3a). Seed the slot with the handle here;
        // the worker claims it on its first iteration and re-deposits it
        // at every park checkpoint so a parked Sim system can drain the
        // per-user tick-buffer via `RecvHandleRes`.
        *self.recv_slot.lock() = Some(recv);

        let recv_tx = self
            .recv_out_chan_tx
            .lock()
            .clone()
            .expect("recv_out_chan_tx Some in Armed state");

        let recv_slot = Arc::clone(&self.recv_slot);
        let shutdown_recv = Arc::clone(&self.shutdown);
        let park_recv = Arc::clone(&self.park);
        let panic_recv = Arc::clone(&self.panic_slot);
        #[cfg(any(test, feature = "test_time"))]
        let test_panic_recv = Arc::clone(&self.test_panic_request);

        #[cfg(feature = "test_time")]
        let clock_handle_recv = naia_bevy_shared::TestClock::shareable_handle();

        let recv_join = thread::Builder::new()
            .name("naia-recv-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::install_shared(clock_handle_recv);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recv_worker_loop(
                        &recv_slot,
                        &recv_tx,
                        &shutdown_recv,
                        &park_recv,
                        #[cfg(any(test, feature = "test_time"))]
                        &test_panic_recv,
                    );
                }));

                if let Err(payload) = result {
                    panic_recv.set(payload);
                }

                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::detach_shared();
            })
            .expect("spawn recv worker thread");
        let recv_thread = recv_join.thread().clone();

        // Send worker borrows the send handle from the shared park-window
        // slot per-iteration (D.3a), symmetric to the recv worker. Seed
        // the slot here; the worker claims it each iteration and
        // re-deposits at every park checkpoint.
        *self.send_slot.lock() = Some(send);

        let snap_rx = self
            .snapshot_receiver
            .lock()
            .take()
            .expect("snapshot_receiver Some in Armed state");

        let send_slot = Arc::clone(&self.send_slot);
        let shutdown_send = Arc::clone(&self.shutdown);
        let park_send = Arc::clone(&self.park);
        let panic_send = Arc::clone(&self.panic_slot);
        #[cfg(any(test, feature = "test_time"))]
        let test_panic_send = Arc::clone(&self.test_panic_request);

        #[cfg(feature = "test_time")]
        let clock_handle_send = naia_bevy_shared::TestClock::shareable_handle();

        let send_join = thread::Builder::new()
            .name("naia-send-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_bevy_shared::TestClock::install_shared(clock_handle_send);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    send_worker_loop(
                        &send_slot,
                        &snap_rx,
                        &shutdown_send,
                        &park_send,
                        #[cfg(any(test, feature = "test_time"))]
                        &test_panic_send,
                    );
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
    /// / [`RecvHandleRes`] / [`SendHandleRes`] without racing the
    /// workers.
    ///
    /// No-op when in `Armed` (workers not yet spawned) or `Stopped`.
    ///
    /// # Panic / exit safety
    ///
    /// A worker that has panicked (its `catch_unwind` body unwound) or
    /// exited will never reach its park checkpoint, so a naive condvar
    /// wait would deadlock. The wait therefore breaks early once every
    /// not-yet-parked worker thread has *finished* — i.e. the live
    /// workers are all parked and the rest are gone. A subsequent
    /// [`Self::propagate_panic_if_any`] surfaces the captured payload.
    /// Callers (e.g. [`drain_recv_worker_output`]) tolerate a
    /// not-fully-parked return by finding `None` in a handle slot and
    /// bailing.
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
            // Break early if the not-yet-parked workers have all
            // finished (panicked/exited) — they will never park.
            let finished = self
                .workers
                .lock()
                .iter()
                .filter(|w| w.join.as_ref().map(|j| j.is_finished()).unwrap_or(true))
                .count() as u32;
            if *g + finished >= expected {
                break;
            }
            // Bounded wait so a worker that finishes *after* the check
            // above (before we re-loop) is still observed promptly.
            self.park
                .parked_cv
                .wait_for(&mut g, std::time::Duration::from_millis(5));
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

    /// Drain the armed coord parking slot into [`CoordHandleRes`] on `world`.
    ///
    /// Called by consumers (e.g. cyberlith E.6c `init.rs`) that drive
    /// Startup manually before running `Update` — the
    /// `drain_armed_into_res` closure registered in `Update` would only
    /// fire after Startup, but `main_init` needs the coord in
    /// `CoordHandleRes` during Startup.  This method reproduces the same
    /// drain logic and is safe to call multiple times (a no-op once the
    /// slot is empty).
    pub fn drain_armed_coord_into_resource(&self, world: &mut bevy_ecs::world::World) {
        if let Some(c) = self.armed_coord.lock().take() {
            world.resource_mut::<CoordHandleRes>().0 = Some(c);
        }
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

    let recv_handle_res = internal.recv_handle_res();
    let send_handle_res = internal.send_handle_res();

    app.insert_resource(sim_converter);
    app.insert_resource(SimEventReceiverRes(sim_event_receiver));
    app.insert_resource(SnapshotSenderRes(snap_sender));
    app.insert_resource(SnapshotReceiverRes(snap_receiver));
    app.insert_resource(CoordHandleRes(None));
    app.insert_resource(send_handle_res);
    app.insert_resource(recv_handle_res);
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

/// Recv worker loop. The `RecvHandle` is **not** owned by this closure
/// — it lives in the shared `recv_slot` (`Arc<Mutex<Option<…>>>`) so a
/// parked-window Sim system can borrow it via [`RecvHandleRes`] for the
/// per-user tick-buffer drain (D.3a).
///
/// Per-iteration discipline that keeps the park-window borrow race-free:
///
/// 1. **Park checkpoint with the handle deposited.** The handle sits in
///    `recv_slot` whenever the worker is at the checkpoint, so once
///    [`PluginInternalState::park_workers`] returns (worker parked +
///    `parked_count` incremented), main is guaranteed to find the handle
///    in the slot.
/// 2. **Claim** the handle from the slot for the brief receive window.
/// 3. `recv.receive()` + ship the `ReceiveOutput`.
/// 4. **Deposit** the handle back into the slot *before* looping back to
///    the checkpoint.
///
/// Because the park flag is only observed at the checkpoint (step 1), a
/// park request that arrives mid-receive-window simply waits one short
/// iteration until the worker has re-deposited the handle and parks.
fn recv_worker_loop(
    recv_slot: &Arc<Mutex<Option<RecvHandle<Entity>>>>,
    out_tx: &Sender<ReceiveOutput<Entity>>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    loop {
        // Park checkpoint: the handle is in `recv_slot`, so a parked Sim
        // system can take it for `receive_tick_buffer_messages`.
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(any(test, feature = "test_time"))]
        if test_panic.load(Ordering::SeqCst) {
            panic!("test-requested recv worker panic");
        }

        // Claim the handle for the brief receive window. If main has it
        // (only possible mid-park, which the checkpoint above already
        // gated), skip this iteration rather than block.
        let mut recv = match recv_slot.lock().take() {
            Some(h) => h,
            None => {
                thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }
        };

        let output = recv.receive();

        // Re-deposit BEFORE any further checkpoint, so the handle is
        // always parkable at the loop top.
        *recv_slot.lock() = Some(recv);

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

/// Send worker loop. Symmetric to [`recv_worker_loop`]: the
/// `SendHandle` lives in the shared `send_slot` so a parked-window Sim
/// system can borrow it via [`SendHandleRes`] for cross-half work that
/// needs the send half. Same per-iteration deposit discipline keeps the
/// park-window borrow race-free.
fn send_worker_loop(
    send_slot: &Arc<Mutex<Option<SendHandle<Entity>>>>,
    snap_rx: &SnapshotReceiver<Entity>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    loop {
        // Park checkpoint: handle is in `send_slot`, borrowable by main.
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(any(test, feature = "test_time"))]
        if test_panic.load(Ordering::SeqCst) {
            panic!("test-requested send worker panic");
        }

        if let Some(snap) = snap_rx.take_latest() {
            // Claim the handle for the send window; if main has it
            // (only mid-park, gated above), skip this iteration.
            let mut send = match send_slot.lock().take() {
                Some(h) => h,
                None => {
                    thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                }
            };
            send.apply_pending_send_preamble();
            send.apply_pending_scope_changes(&snap);
            send.send_all_packets(snap);
            // Re-deposit before looping back to the park checkpoint.
            *send_slot.lock() = Some(send);
        } else {
            // No snapshot pending — short sleep so we don't hot-spin.
            thread::sleep(std::time::Duration::from_micros(100));
        }
    }
}

// ─── Main-side drain system ────────────────────────────────────────────────

/// Bevy system installed on the consumer Sim app. Drains
/// `ReceiveOutput<Entity>` from the Recv worker's bounded channel, runs
/// the cross-half receive orchestration against the Sim world, and fans
/// the resulting events into the bevy `Messages<X>` buffers + the
/// `SimEventReceiver`.
///
/// Runs in `Update` by default (same as `process_packets` for the
/// non-`sim_integration_full` variants).
///
/// # D.3a — full cross-half orchestration
///
/// The recv worker only runs `recv.receive()` (a recv-half socket drain
/// with no `&mut World` and no `SendHandle`), so the `ReceiveOutput` it
/// ships carries handshake-time world events plus the still-undecoded
/// `received_addresses` + `pending_data_packets`. Decoding those data
/// packets — which produces client-originated Spawn/Insert/Update world
/// events AND fills each connection's per-user tick-buffer (the source
/// of `receive_tick_buffer_messages`) — requires the cross-half
/// [`pipeline_actors::apply_recv_to_world`] step, which needs `&mut
/// World` + the coord + recv + send handles.
///
/// This system therefore:
/// 1. Parks the workers (so the recv + send handles are deposited in
///    their park-window slots and reachable on main, race-free).
/// 2. Takes coord (from [`CoordHandleRes`]) + recv (from the
///    [`RecvHandleRes`] slot) + send (from the [`SendHandleRes`] slot).
/// 3. Runs `apply_recv_to_world` per `ReceiveOutput` (handshake
///    finalization + `process_recv_packets` decode + `process_all_packets`),
///    then `apply_receive_output_pipeline_with_sim_receiver` to fan the
///    combined event set into bevy `Messages<X>` + the `SimEventReceiver`.
/// 4. Returns the handles to their slots/Resource and unparks.
///
/// After this system runs, a *subsequent* Sim system can take the
/// `RecvHandle` (via [`RecvHandleRes`], parking again) and call
/// [`naia_server::RecvHandle::receive_tick_buffer_messages`] for each
/// `TickEvent` to drain the now-decoded per-user tick-buffered messages
/// (e.g. cyberlith's `PlayerCommands`). That tick-buffer drain is the
/// consumer's responsibility (cyberlith Phase E.6) — naia only exposes
/// the handle.
pub fn drain_recv_worker_output(world: &mut World) {
    use naia_bevy_shared::WorldProxyMut;
    use naia_server::pipeline_actors::apply_recv_to_world;

    // Pull the channel + sim_event_receiver without holding an outer
    // borrow on world.
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

    // Park the workers so the recv + send handles are reachable on main,
    // then take all three handles for the cross-half orchestration.
    // (`park_workers` is a no-op in Armed/Stopped, in which case the
    // handle slots are not yet populated and we bail out cleanly.)
    let (recv_slot, send_slot) = {
        let state = world.resource::<PluginInternalState>();
        state.park_workers();
        (
            Arc::clone(&world.resource::<RecvHandleRes>().0),
            Arc::clone(&world.resource::<SendHandleRes>().0),
        )
    };

    let coord_opt = world.resource_mut::<CoordHandleRes>().0.take();
    let recv_opt = recv_slot.lock().take();
    let send_opt = send_slot.lock().take();

    if coord_opt.is_none() || recv_opt.is_none() || send_opt.is_none() {
        // A handle was in use elsewhere this frame (or workers not
        // running). Put back whatever we took and unpark.
        if let Some(c) = coord_opt {
            world.resource_mut::<CoordHandleRes>().0 = Some(c);
        }
        if let Some(r) = recv_opt {
            *recv_slot.lock() = Some(r);
        }
        if let Some(s) = send_opt {
            *send_slot.lock() = Some(s);
        }
        world.resource::<PluginInternalState>().unpark_workers();
        return;
    }

    // Cross-half receive orchestration per ReceiveOutput.
    let mut coord = coord_opt.unwrap();
    let mut recv = recv_opt.unwrap();
    let mut send = send_opt.unwrap();
    for mut output in outputs {
        let server_tick = coord.current_tick();
        let (c, r, s) = apply_recv_to_world(
            coord,
            recv,
            send,
            world.proxy_mut(),
            &mut output,
            server_tick,
        );
        coord = c;
        recv = r;
        send = s;
        apply_receive_output_pipeline_with_sim_receiver(world, &coord, &sim_receiver, output);
    }

    // Return the handles and resume the workers.
    world.resource_mut::<CoordHandleRes>().0 = Some(coord);
    *recv_slot.lock() = Some(recv);
    *send_slot.lock() = Some(send);
    world.resource::<PluginInternalState>().unpark_workers();
}

/// Bevy system that surfaces any worker panic onto the main thread.
/// Installed in the same default schedule as `drain_recv_worker_output`.
pub fn propagate_worker_panics(state: bevy_ecs::system::Res<PluginInternalState>) {
    state.propagate_panic_if_any();
}
