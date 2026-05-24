//! `Plugin::sim_integration_full` — Plugin variant that internally
//! owns the Recv + Send worker threads + the three pipeline handles
//! (`SimHandle` / `RecvHandle` / `SendHandle`).
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
//! - [`SimHandleRes`] — Sim systems take/return for cross-handle work
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
    thread::{self, JoinHandle},
};

use bevy_app::App;
use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    schedule::{InternedScheduleLabel, IntoScheduleConfigs, ScheduleLabel},
    world::World,
};
use crossbeam_channel::{bounded, Receiver, Sender};
// `TrySendError` is only used on the active receive path (the parked recv
// worker is a pure parking service and never sends output).
#[cfg(workers_active)]
use crossbeam_channel::TrySendError;
use parking_lot::Mutex;

use naia_bevy_shared::Protocol as BevyProtocol;
use naia_server::{
    pipeline_actors::{
        spawn_server_handles, SimHandle, SimEventReceiver, SnapshotReceiver, SnapshotSender,
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
}

impl PluginSimConfig {
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
}

// ─── Resources ──────────────────────────────────────────────────────────────

/// Bevy resource holding the [`SimHandle`]. Take/return pattern: Sim
/// systems use [`Option::take`] to gain ownership for cross-handle ops
/// (`configure_entity_replication`, `send_message_to_user`, etc.) and
/// must put the handle back before returning.
#[derive(Resource)]
pub struct SimHandleRes(pub Option<SimHandle<Entity>>);

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
/// [`SimHandleRes`])
///
/// The [`SimHandle`] lives **only** on main — no worker ever owns it —
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
    /// Signalled by a worker when it increments `parked_count` at the
    /// checkpoint. `park_workers()` waits on this for `parked_count`
    /// to reach the expected worker count.
    parked_cv: parking_lot::Condvar,
    /// Signalled by `unpark_workers()` (under the `parked_count` lock)
    /// when it clears `park`. Parked workers wait on this to leave the
    /// checkpoint. Replaces the prior `thread::park()`/`thread::unpark()`
    /// scheme, which had a token race: a worker that consumed its unpark
    /// token but was descheduled before re-reading `park` would re-read
    /// the *next* cycle's `park=true` and re-block with no token, never
    /// re-incrementing `parked_count` → deadlock. A condvar guarded by
    /// the same mutex as the `park` read has no such lost transition.
    resume_cv: parking_lot::Condvar,
    /// Signalled by a worker when it decrements `parked_count` on leaving
    /// the checkpoint. `unpark_workers()` waits on this until
    /// `parked_count` is back to 0 — i.e. it is **synchronous**: it does
    /// not return until every parked worker has observed `park=false` and
    /// left the checkpoint. Because `unpark_workers()` and the next
    /// `park_workers()` run sequentially on the same thread, this
    /// guarantees `park` can never be re-set to `true` while a worker is
    /// still mid-resume.
    resumed_cv: parking_lot::Condvar,
    /// Mutex + condvar for workers to sleep in between park windows. Workers
    /// block here when idle; `park_workers()` signals this condvar after setting
    /// `park=true` so workers wake, loop to `worker_park_checkpoint`, and enter
    /// the coordinated park protocol.
    ///
    /// Used by BOTH paths (MISSION_OVERLAP_FRONTIER T1 made it active-mode too):
    ///   - not(workers_active): the worker has no other work, so it body-sleeps
    ///     on an UNBOUNDED `wait()` until woken here.
    ///   - workers_active: the worker idle-waits on a BOUNDED `wait_for(100µs)`
    ///     between receive/send iterations; signalling here wakes it instantly
    ///     instead of after up to one ~100µs poll (the park-barrier win).
    ///
    /// This is the *idle* wait, separate from the *checkpoint* wait
    /// (`resume_cv`): a body-sleeping worker isn't yet counted in
    /// `parked_count`, whereas a checkpoint-waiting worker is. Keeping them
    /// distinct lets `park_workers()` wake idlers (body_sleep_cv) without
    /// disturbing the parked-count barrier, and lets `unpark_workers()`
    /// release parked workers (resume_cv) without waking idlers.
    body_sleep_mu: Mutex<()>,
    body_sleep_cv: parking_lot::Condvar,
    /// Event-driven control wake (workers_active): an awaitable, coalescing
    /// `bounded(1)` signal the recv worker selects on alongside the
    /// transport's packet-readiness. Every site that must wake an idle
    /// worker — `park_workers`, `Drop` (shutdown), test-panic — pings it
    /// via [`Self::ping_control`]. This is the async sibling of
    /// `body_sleep_cv`: the active worker no longer polls a 100µs condvar,
    /// so park/shutdown can no longer be observed "within one poll" — they
    /// must explicitly wake it. The `not(workers_active)` (deterministic)
    /// path still uses `body_sleep_cv` and ignores this.
    control_tx: smol::channel::Sender<()>,
    // Only read by the event-driven worker select (workers_active); the
    // deterministic path waits on `body_sleep_cv` instead.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    control_rx: smol::channel::Receiver<()>,
}

impl ParkControl {
    fn new() -> Self {
        // bounded(1) ⇒ coalescing: at most one pending wake token.
        let (control_tx, control_rx) = smol::channel::bounded(1);
        Self {
            park: AtomicBool::new(false),
            parked_count: Mutex::new(0),
            parked_cv: parking_lot::Condvar::new(),
            resume_cv: parking_lot::Condvar::new(),
            resumed_cv: parking_lot::Condvar::new(),
            body_sleep_mu: Mutex::new(()),
            body_sleep_cv: parking_lot::Condvar::new(),
            control_tx,
            control_rx,
        }
    }

    /// Wake the event-driven recv worker (coalescing; drop-if-full and
    /// drop-if-closed are both fine — a buffered token already guarantees
    /// the wake, and a closed channel means the worker is gone).
    fn ping_control(&self) {
        let _ = self.control_tx.try_send(());
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
    /// `(sim_handle, recv, send)` parked here between `build` and `listen`.
    /// `None` after `listen` (handles moved into workers / Resources).
    armed_handles: Mutex<Option<(SimHandle<Entity>, RecvHandle<Entity>, SendHandle<Entity>)>>,
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
    /// `drain_armed_sim_handle_into_resource` system to install into
    /// [`SimHandleRes`]. Drained at most once.
    armed_sim_handle: Mutex<Option<SimHandle<Entity>>>,
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
    join: Option<JoinHandle<()>>,
}

impl PluginInternalState {
    fn new_armed(
        sim_handle: SimHandle<Entity>,
        recv: RecvHandle<Entity>,
        send: SendHandle<Entity>,
        sim_event_receiver: SimEventReceiver<Entity>,
        snapshot_sender: SnapshotSender<Entity>,
        snapshot_receiver: SnapshotReceiver<Entity>,
    ) -> Self {
        let (tx, rx) = bounded::<ReceiveOutput<Entity>>(1);
        Self {
            state: AtomicU8::new(State::Armed as u8),
            armed_handles: Mutex::new(Some((sim_handle, recv, send))),
            recv_out_chan_rx: Mutex::new(Some(rx)),
            recv_out_chan_tx: Mutex::new(Some(tx)),
            snapshot_receiver: Mutex::new(Some(snapshot_receiver)),
            _snapshot_sender_keep: Mutex::new(Some(snapshot_sender)),
            shutdown: Arc::new(AtomicBool::new(false)),
            park: Arc::new(ParkControl::new()),
            panic_slot: Arc::new(PanicSlot::new()),
            workers: Mutex::new(Vec::new()),
            sim_event_receiver: Mutex::new(Some(sim_event_receiver)),
            armed_sim_handle: Mutex::new(None),
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
        // Wake any parked or idle (body-sleeping) workers so they loop
        // back to the top and observe the request. (Polling workers in
        // the non-test_time path observe it on their next iteration
        // without a wakeup.)
        self.park.resume_cv.notify_all();
        #[cfg(not(workers_active))]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Wake the event-driven (workers_active) worker so it loops back and
        // observes the panic request.
        self.park.ping_control();
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

        let (sim_handle, recv, send) = self
            .armed_handles
            .lock()
            .take()
            .expect("armed_handles Some in Armed state");

        // Load socket io via the run_with_world_server reassembly
        // helper. Byte-for-byte equivalent to `Server::listen`.
        let socket: Box<dyn naia_server::transport::Socket> = socket.into();
        let (_a, _b, ps, pr) = naia_server::transport::Socket::listen(socket);
        let (sim_handle, recv, send, ()) =
            naia_server::pipeline_actors::run_with_world_server(sim_handle, recv, send, |ws| {
                ws.io_load(ps, pr);
            });

        // Extract the transport's event-driven readiness (Some for the
        // in-process PacketChannel every cell uses; None for poll-only
        // sockets) before the handle is deposited — the recv worker selects
        // on it instead of polling.
        let recv_readiness = recv.readiness();

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

        // Create a single shared clock Arc for ALL workers. This ensures
        // TestClock::advance on the calling thread (scenario test thread)
        // advances the same clock that both workers read. Two separate
        // shareable_handle() calls would create two distinct Arcs — the
        // calling thread would only be linked to the last one.
        #[cfg(feature = "test_time")]
        let clock_handle_shared = naia_bevy_shared::TestClock::shareable_handle();
        #[cfg(feature = "test_time")]
        let clock_handle_recv = Arc::clone(&clock_handle_shared);

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
                        recv_readiness,
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
        let clock_handle_send = Arc::clone(&clock_handle_shared);

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

        // Coord handle stays on main as a Resource via SimHandleRes
        // — the caller's `Plugin::build` already installed
        // `SimHandleRes(None)` and will fill it from a Startup
        // system that drains `armed_sim_handle` here.
        //
        // To avoid a chicken-and-egg with Startup ordering, the
        // sim_integration_full constructor installs a Startup system
        // (`drain_armed_sim_handle`) that runs once and pulls the parked
        // SimHandle into the resource. Since `listen()` can be
        // called before or after Startup, we stash the SimHandle on the
        // PluginInternalState itself for that drain.
        self.armed_sim_handle.lock().replace(sim_handle);

        self.workers.lock().extend([
            WorkerHandle {
                name: "naia-recv-worker",
                join: Some(recv_join),
            },
            WorkerHandle {
                name: "naia-send-worker",
                join: Some(send_join),
            },
        ]);

        self.state.store(State::Running as u8, Ordering::Release);
    }

    /// Park both worker threads (block until both reach the top of
    /// their idle loop). After return, callers may safely
    /// `TestClock::advance(...)` or borrow handles via [`SimHandleRes`]
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
        // Wake body-sleeping workers via condvar so they loop back to the
        // checkpoint and park immediately, on BOTH paths:
        //   - Parked (deterministic) mode: the worker body-sleeps on an
        //     UNBOUNDED `body_sleep_cv.wait()` until woken (it has no other work).
        //   - Active mode (MISSION_OVERLAP_FRONTIER T1): the worker idle-waits on
        //     a BOUNDED `body_sleep_cv.wait_for(100µs)` between receive/send
        //     iterations. Signalling here wakes it instantly instead of after up
        //     to one ~100µs poll interval — that per-worker poll latency was the
        //     ~140µs park barrier (audit §2.4). The 100µs bound still re-polls the
        //     socket between ticks if no park is pending (preserves receive cadence).
        //
        // Hold body_sleep_mu while notifying to prevent the classic lost-wakeup
        // race: without the lock, notify_all() can fire AFTER a worker checks the
        // condvar condition (finds park=false) but BEFORE it calls wait() — the
        // signal is lost. Holding the lock (which the worker also holds across its
        // park-check + wait) ensures either the worker sees park=true (set above,
        // SeqCst) and skips the wait, or it is already inside wait() and is woken.
        // (parking_lot condvar wait releases the lock atomically.)
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Event-driven (workers_active) path: the active worker blocks in a
        // `future::or` rather than the 100µs condvar poll, so wake it
        // explicitly. The bounded(1) token is buffered if it's mid-drain
        // (no lost wakeup); `park` is already SeqCst-set above so the woken
        // worker observes it at the checkpoint.
        self.park.ping_control();
        // Time spent blocked here is the "park barrier wait" — the main thread
        // waiting for the workers to reach their checkpoint. Large per-tick
        // ⇒ the workers' tick work landed on the main critical path
        // (serialization); ~0 ⇒ they finished in the gaps. See pipeline_timing.
        #[cfg(feature = "pipeline_timing")]
        let _t_barrier = std::time::Instant::now();
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
        #[cfg(feature = "pipeline_timing")]
        crate::pipeline_timing::record_barrier(_t_barrier.elapsed().as_nanos() as u64);
    }

    /// Resume both worker threads. **Synchronous**: does not return until
    /// every parked worker has observed `park=false` and left the
    /// checkpoint (`parked_count` back to 0). This is what makes the next
    /// `park_workers()` safe — it cannot set `park=true` while a worker is
    /// still mid-resume, which was the root of the prior `thread::park()`
    /// token-race deadlock.
    pub fn unpark_workers(&self) {
        if self.state() != State::Running {
            return;
        }
        let mut g = self.park.parked_count.lock();
        // Clear the park flag and wake checkpoint waiters under the same
        // lock the checkpoint reads `park` under (no lost wakeup).
        self.park.park.store(false, Ordering::SeqCst);
        self.park.resume_cv.notify_all();
        // Wait until all parked workers have decremented out of the
        // checkpoint. A worker that finished/panicked never incremented,
        // so it doesn't hold up the count.
        while *g > 0 {
            // Bounded wait so a worker that finishes (panic/exit) after we
            // last observed the count is still noticed promptly.
            let finished_unparked = self
                .workers
                .lock()
                .iter()
                .filter(|w| w.join.as_ref().map(|j| j.is_finished()).unwrap_or(true))
                .count() as u32;
            // If the only thing keeping the count above 0 would be a
            // worker that has since finished, stop waiting.
            if finished_unparked >= *g {
                break;
            }
            self.park
                .resumed_cv
                .wait_for(&mut g, std::time::Duration::from_millis(5));
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

    /// Take the armed sim_handle from the parking slot.  Filled by `listen()`
    /// and drained by the Startup `drain_armed_sim_handle_into_resource` system
    /// or by consumers that need to avoid a combined `&World`/`&mut World`
    /// borrow (e.g. cyberlith `init.rs` where the same `World` provides
    /// both `resource::<PluginInternalState>()` and the target
    /// `resource_mut::<SimHandleRes>()`).
    ///
    /// Returns `None` once the slot has been drained (idempotent).
    pub fn take_armed_sim_handle(&self) -> Option<SimHandle<Entity>> {
        self.armed_sim_handle.lock().take()
    }

    /// Drain the armed sim_handle parking slot into [`SimHandleRes`] on `world`.
    ///
    /// Called by consumers (e.g. cyberlith E.6c `init.rs`) that drive
    /// Startup manually before running `Update` — the
    /// `drain_armed_into_res` closure registered in `Update` would only
    /// fire after Startup, but `main_init` needs the SimHandle in
    /// `SimHandleRes` during Startup.  This method reproduces the same
    /// drain logic and is safe to call multiple times (a no-op once the
    /// slot is empty).
    pub fn drain_armed_sim_handle_into_resource(&self, world: &mut bevy_ecs::world::World) {
        if let Some(c) = self.armed_sim_handle.lock().take() {
            world.resource_mut::<SimHandleRes>().0 = Some(c);
        }
    }

    /// Obtain a [`naia_server::pipeline_actors::SendStateView`] from the
    /// pre-`listen()` armed SimHandle.
    ///
    /// Called by consumers (e.g. cyberlith E.6c `init.rs`) that need to
    /// pass `send_state_view` to `install_sim_plugins` before `listen()`
    /// is invoked.  Must only be called while in the `Armed` state
    /// (before `listen()`); panics if the armed handles have already been
    /// consumed by `listen()`.
    pub fn armed_send_state_view(
        &self,
    ) -> naia_server::pipeline_actors::SendStateView<Entity> {
        let guard = self.armed_handles.lock();
        let (sim_handle, _, _) = guard.as_ref()
            .expect("armed_send_state_view called after listen() — handles already moved");
        sim_handle.send_state_view()
    }
}

impl Drop for PluginInternalState {
    fn drop(&mut self) {
        // Signal shutdown.
        self.shutdown.store(true, Ordering::SeqCst);
        // Drop the recv channel sender so the main-side drain stops
        // expecting more outputs.
        self.recv_out_chan_tx.lock().take();
        // Clear the park flag + wake checkpoint waiters so any parked
        // worker leaves the checkpoint and observes shutdown. Done under
        // the parked_count lock (same lock the checkpoint reads `park`
        // under) to avoid a lost wakeup.
        {
            let _g = self.park.parked_count.lock();
            self.park.park.store(false, Ordering::SeqCst);
            self.park.resume_cv.notify_all();
        }
        // Wake body-sleeping workers (parked-worker mode) so they observe
        // shutdown. Hold the lock to prevent lost-wakeup (see park_workers).
        #[cfg(not(workers_active))]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Wake the event-driven (workers_active) recv worker so it leaves its
        // `future::or` and observes `shutdown` at the loop top — otherwise it
        // would block until the 5s join deadline.
        self.park.ping_control();
        let mut workers = std::mem::take(&mut *self.workers.lock());

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
/// `SnapshotReceiver`, `SimHandleRes`, `SendHandleRes`,
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
    let (sim_handle, recv, send) = spawn_server_handles::<Entity, _>(server_config, naia_proto);

    let sim_converter = SimConverter::from_sim(&sim_handle);
    let sim_event_receiver = SimEventReceiver::<Entity>::new();
    let (snap_sender, snap_receiver) = SnapshotSender::<Entity>::pair();

    let internal = PluginInternalState::new_armed(
        sim_handle,
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
    app.insert_resource(SimHandleRes(None));
    app.insert_resource(send_handle_res);
    app.insert_resource(recv_handle_res);
    app.insert_resource(internal);

    // Startup: drain the armed sim_handle into SimHandleRes once the
    // consumer's `listen()` has run. The system itself is a no-op
    // until listen completes — re-run via the normal Update schedule
    // is fine because once drained the field is `None`.
    let drain_armed_into_res = |world: &mut World| {
        let sim_handle_opt = world
            .get_resource::<PluginInternalState>()
            .and_then(|s| s.armed_sim_handle.lock().take());
        if let Some(c) = sim_handle_opt {
            world.resource_mut::<SimHandleRes>().0 = Some(c);
        }
    };

    // Register main-side systems in the consumer's schedule (Update
    // by default). drain_armed runs FIRST so subsequent systems see
    // the SimHandle; drain_recv_worker_output then applies output; panic
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
/// main, then wait on `resume_cv` until `unpark_workers()` clears `park`.
fn worker_park_checkpoint(park: &ParkControl) {
    // Fast path: not parking, no lock needed.
    if !park.park.load(Ordering::SeqCst) {
        return;
    }
    let mut g = park.parked_count.lock();
    // Re-check under the lock: `park` may have been cleared between the
    // unlocked load above and acquiring the lock.
    if !park.park.load(Ordering::SeqCst) {
        return;
    }
    *g += 1;
    park.parked_cv.notify_all();
    // Park loop: wait on the resume condvar (NOT thread::park) until the
    // park flag clears. The condvar wait atomically releases `g`; the
    // `park` read happens under `g`, and `unpark_workers()` clears `park`
    // + notifies under the same lock — so there is no lost transition and
    // no token to drop.
    while park.park.load(Ordering::SeqCst) {
        park.resume_cv.wait(&mut g);
    }
    // Leaving the checkpoint: decrement and signal `unpark_workers()`,
    // which blocks until this count returns to 0 (synchronous unpark).
    *g -= 1;
    park.resumed_cv.notify_all();
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
//
// `recv_slot` / `out_tx` are only touched on the active receive path; the
// parked recv worker is a pure parking service.
#[cfg_attr(not(workers_active), allow(unused_variables))]
fn recv_worker_loop(
    recv_slot: &Arc<Mutex<Option<RecvHandle<Entity>>>>,
    out_tx: &Sender<ReceiveOutput<Entity>>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    readiness: Option<naia_server::transport::PacketReadiness>,
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
        // Claim the handle for the brief receive window using `try_lock` so
        // the worker always loops back to the park checkpoint rather than
        // blocking indefinitely. Blocking `.lock()` could cause the recv
        // worker to be stuck waiting for the lock while `park_workers()` waits
        // for this worker to reach its checkpoint — a deadlock. `try_lock`
        // means a contended slot (held briefly by `open_park_window` /
        // `drain_recv_worker_output`) just produces a 100µs retry; the
        // worker then hits the park checkpoint on the next iteration.
        // In test_time mode (LocalTransport, deterministic clock), the main
        // thread drives all recv.receive() calls synchronously in
        // drain_recv_worker_output while workers are parked. Running
        // recv.receive() here concurrently causes RwLock contention on
        // time_manager (recv needs write; send_all_packets holds read), which
        // can deadlock park_workers() when the recv worker is blocked inside
        // receive() and cannot reach its park checkpoint.
        //
        // Production (non-test_time): recv.receive() runs here to drain the
        // UDP socket promptly between ticks, avoiding packet loss under load.
        #[cfg(not(workers_active))]
        {
            // Body-sleep: block with zero CPU cost until park_workers()
            // signals body_sleep_cv (meaning park=true and workers should
            // reach the checkpoint). Using body_sleep_cv instead of
            // thread::park() avoids the single-token race where park_workers()'s
            // thread::unpark() token is consumed here, leaving the checkpoint
            // while-loop's thread::park() without a token (deadlock). Condvar
            // notify_all is not token-based and always wakes waiting workers.
            // The `test_panic` clause lets `request_worker_panic_for_test`
            // wake an idle worker so it loops back and observes the request.
            let mut g = park.body_sleep_mu.lock();
            while !park.park.load(Ordering::SeqCst)
                && !shutdown.load(Ordering::SeqCst)
                && !test_panic.load(Ordering::SeqCst)
            {
                park.body_sleep_cv.wait(&mut g);
            }
            drop(g);
            continue;
        }

        #[cfg(workers_active)]
        {
        // Don't start a receive() if park was requested — loop straight back to
        // the checkpoint (which parks immediately). No sleep: the worker is about
        // to park, and a sleep here would just add up to 100µs to the barrier in
        // the race window where park is set after the checkpoint returned. (T1)
        if park.park.load(Ordering::SeqCst) {
            continue;
        }

        let mut recv = match recv_slot.try_lock().and_then(|mut g| g.take()) {
            Some(h) => h,
            None => {
                thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }
        };

        #[cfg(feature = "pipeline_timing")]
        let _t_recv = std::time::Instant::now();
        let output = recv.receive();
        #[cfg(feature = "pipeline_timing")]
        crate::pipeline_timing::record_recv(_t_recv.elapsed().as_nanos() as u64);

        // Re-deposit using try_lock spin (never blocks park checkpoint).
        loop {
            match recv_slot.try_lock() {
                Some(mut g) => { *g = Some(recv); break; }
                None => { thread::sleep(std::time::Duration::from_micros(100)); }
            }
        }

        // bounded(1) channel — drop newer output if full.
        match out_tx.try_send(output) {
            Ok(()) => {}
            Err(TrySendError::Full(new_output)) => { drop(new_output); }
            Err(TrySendError::Disconnected(_)) => { return; }
        }
        // Idle inter-iteration wait.
        match &readiness {
            // Event-driven (in-process PacketChannel): block with ZERO CPU
            // until either the transport signals a packet may be ready OR a
            // control wake (park / shutdown / test-panic) fires. This
            // eliminates the 100µs idle-poll storm: under oversubscription
            // the cell is fed only ~once per 5ms service loop, so ~98% of
            // the old polls were wasted wakeups.
            //
            // Lost-wakeup-free: async-channel `recv()` re-checks the buffer
            // before parking, and both signals are bounded(1) coalescing —
            // a ping that lands between this worker's drain and its next
            // wait leaves a buffered token, so the freshly-created future
            // resolves immediately. The per-tick park-window drain remains
            // the authoritative safety net (≤1-tick dwell ceiling).
            Some(readiness) => {
                if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                    smol::block_on(smol::future::or(readiness.wait(), async {
                        let _ = park.control_rx.recv().await;
                    }));
                }
                // Clear the token(s) that woke us so the next iteration's
                // wait starts fresh (data is drained by recv.receive() at
                // the loop top; any packet that arrives after this re-pings).
                readiness.drain();
                while park.control_rx.try_recv().is_ok() {}
            }
            // Poll-only transport (raw socket, no awaitable readiness): keep
            // the bounded condvar poll — park_workers()'s body_sleep_cv wake
            // still cuts the park barrier; the 100µs bound drains the socket.
            None => {
                let mut g = park.body_sleep_mu.lock();
                if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                    park.body_sleep_cv
                        .wait_for(&mut g, std::time::Duration::from_micros(100));
                }
                drop(g);
            }
        }
        } // end #[cfg(workers_active)]
    }
}

/// Send worker loop. Symmetric to [`recv_worker_loop`]: the
/// `SendHandle` lives in the shared `send_slot` so a parked-window Sim
/// system can borrow it via [`SendHandleRes`] for cross-half work that
/// needs the send half. Same per-iteration deposit discipline keeps the
/// park-window borrow race-free.
///
/// In the parked (deterministic) mode this worker is a **pure parking
/// service** (like the recv worker): the consumer drives the send
/// synchronously inside its park window, so `snap_rx` / `send_slot` are
/// unused here.
#[cfg_attr(not(workers_active), allow(unused_variables))]
fn send_worker_loop(
    send_slot: &Arc<Mutex<Option<SendHandle<Entity>>>>,
    snap_rx: &SnapshotReceiver<Entity>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    // MISSION_TICK_FLOOR Lever 3: one-tick send lag. The job published this
    // cycle is buffered here and transmitted on the NEXT cycle, so the worker
    // sends the previous tick's frozen job. With the park still in place (L3.2)
    // this validates the frozen send produces correct (1-tick-late) wire vs the
    // oracle; once the park is removed (L3.4) the lag is what lets the transmit
    // overlap the next tick's Sim safely (the frozen rider prevents a torn read
    // of the live `global_dirty`).
    #[cfg(workers_active)]
    let mut held_job: Option<crate::SnapshotWorld<Entity>> = None;
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

        // In test_time the send worker is a PURE PARKING SERVICE: the consumer
        // drives the send (preamble + scope changes + send_all_packets)
        // synchronously inside its park window, so handshake-response and
        // snapshot delivery occur at a deterministic point each tick rather than
        // whenever this real-time thread happens to be scheduled. That real-time
        // timing was load-dependent and could reorder connect handshakes under
        // parallel test load, perturbing avatar spawn order (rapier handle
        // assignment) → non-deterministic physics. Body-sleep until the next
        // park (zero CPU); never touch the snapshot or the send handle.
        #[cfg(not(workers_active))]
        {
            let mut g = park.body_sleep_mu.lock();
            while !park.park.load(Ordering::SeqCst)
                && !shutdown.load(Ordering::SeqCst)
                && !test_panic.load(Ordering::SeqCst)
            {
                park.body_sleep_cv.wait(&mut g);
            }
            drop(g);
            continue;
        }

        #[cfg(workers_active)]
        {
            // Don't start send work if park was requested — send_all_packets
            // holds time_manager.read() for its entire packet loop, which can
            // block the recv worker's time_manager.write() (in take_tick_events);
            // skipping lets this worker reach its checkpoint without contention.
            // Loop straight back to the checkpoint (no sleep): the worker is about
            // to park, and a sleep here would just add to the barrier wait. (T1)
            if park.park.load(Ordering::SeqCst) {
                continue;
            }

            // Claim the handle FIRST — before touching the lag buffer — so a
            // failed claim cannot drop a buffered job. try_lock never blocks the
            // park checkpoint.
            let mut send = match send_slot.try_lock().and_then(|mut g| g.take()) {
                Some(h) => h,
                None => {
                    thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                }
            };

            // MISSION_TICK_FLOOR Lever 3: one-tick lag. Buffer this cycle's
            // freshly-published job; transmit the job buffered on the PREVIOUS
            // cycle. `replace` returns the prior buffered job (to send now);
            // when no new job was published, flush whatever is still buffered.
            let job_to_send = match snap_rx.take_latest() {
                Some(new_job) => held_job.replace(new_job),
                None => held_job.take(),
            };

            // MISSION_SNAPSHOT_DIRTY_TRIM (2026-05-20): the preamble + scope
            // application + needed-set refresh run on the MAIN thread inside the
            // park window (cyberlith `do_park_window_tick` Step 7.5), BEFORE the
            // snapshot is built — so the snapshot contains exactly what this send
            // reads. The worker only transmits.
            #[cfg(feature = "pipeline_timing")]
            let _t_send = std::time::Instant::now();

            if let Some(mut job) = job_to_send {
                // MISSION_TICK_FLOOR Lever 3: dispatch on the prepared send plan.
                // An active job carries a `SendPlan` built on MAIN at the freeze
                // point (frozen `DiffMask`s + frozen dirty domain + live masks
                // already cleared), so this lagged transmit reads ZERO live
                // per-user diff state. A job without a plan (not expected on this
                // path) falls back to a synchronous prepare+transmit, which is
                // only consistent if the live bitset isn't being mutated
                // concurrently — i.e. while the park still serializes (L3.2).
                let _ = job.take_frozen_dirty();
                match job.take_send_plan() {
                    Some(plan) => {
                        // L3 seam Step 5: drain the ACK channel in the worker
                        // preamble (before transmit) so `sent_updates` is
                        // consumed on the send side — single-owner. The no-plan
                        // fallback below drains inside `send_all_packets`.
                        send.drain_all_acks();
                        send.transmit_send_job(job, plan);
                    }
                    None => send.send_all_packets(job),
                }
                #[cfg(feature = "pipeline_timing")]
                crate::pipeline_timing::record_send(_t_send.elapsed().as_nanos() as u64);

                // Re-deposit before looping back to the park checkpoint.
                loop {
                    match send_slot.try_lock() {
                        Some(mut g) => { *g = Some(send); break; }
                        None => { thread::sleep(std::time::Duration::from_micros(100)); }
                    }
                }
            } else {
                // Nothing buffered yet (one-tick warmup) or nothing to send:
                // re-deposit and bounded idle-wait so park_workers() wakes us
                // instantly; the 100µs bound re-polls for the next job. (T1)
                loop {
                    match send_slot.try_lock() {
                        Some(mut g) => { *g = Some(send); break; }
                        None => { thread::sleep(std::time::Duration::from_micros(100)); }
                    }
                }
                {
                    let mut g = park.body_sleep_mu.lock();
                    if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                        park.body_sleep_cv
                            .wait_for(&mut g, std::time::Duration::from_micros(100));
                    }
                    drop(g);
                }
                continue;
            }
        }
    }
}

// ─── Main-side drain system ────────────────────────────────────────────────

/// Core receive drain logic: drains `ReceiveOutput` from the channel
/// and from a synchronous `recv.receive()` call, applies cross-half
/// orchestration, and fans events into the bevy `Messages<X>` buffers
/// + the `SimEventReceiver`.
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
    use naia_bevy_shared::WorldProxyMut;
    use naia_server::pipeline_actors::apply_recv_to_world;

    let receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.recv_out_chan_rx.lock().as_ref().cloned());
    let Some(receiver) = receiver else { return };

    let sim_receiver = world
        .get_resource::<PluginInternalState>()
        .and_then(|s| s.sim_event_receiver.lock().as_ref().cloned());
    let Some(sim_receiver) = sim_receiver else { return };

    let sim_handle_opt = world.resource_mut::<SimHandleRes>().0.take();
    let recv_opt = recv_slot.lock().take();
    let send_opt = send_slot.lock().take();

    if sim_handle_opt.is_none() || recv_opt.is_none() || send_opt.is_none() {
        if let Some(c) = sim_handle_opt {
            world.resource_mut::<SimHandleRes>().0 = Some(c);
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

    // Drain everything the recv worker has already shipped to the channel
    // (bounded(1), so at most one item from prior iterations).
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
        world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
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
        let (c, r, s) = apply_recv_to_world(
            sim_handle,
            recv,
            send,
            world.proxy_mut(),
            &mut output,
            server_tick,
        );
        sim_handle = c;
        recv = r;
        send = s;
        apply_receive_output_pipeline_with_sim_receiver(world, &sim_handle, &sim_receiver, output);
    }
    #[cfg(feature = "pipeline_timing")]
    crate::pipeline_timing::record_apply(_t_apply.elapsed().as_nanos() as u64);

    // Return the handles (no unpark — caller's responsibility).
    world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
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
