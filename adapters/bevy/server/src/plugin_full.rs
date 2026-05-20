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
// `TrySendError` is only used on the non-test_time receive path (the
// test_time recv worker is a pure parking service and never sends output).
#[cfg(not(feature = "test_time"))]
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
}

impl PluginSimConfig {
    /// Set the change-detection schedule label.
    pub fn with_schedule<S: ScheduleLabel>(mut self, schedule: S) -> Self {
        self.change_detection_schedule = Some(schedule.intern());
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
    /// Mutex + condvar for workers to sleep in between park windows
    /// (test_time mode only). Workers block here when idle; `park_workers()`
    /// signals this condvar after setting `park=true` so workers wake, loop
    /// to `worker_park_checkpoint`, and enter the coordinated park protocol.
    ///
    /// This is the *idle* wait, separate from the *checkpoint* wait
    /// (`resume_cv`): a body-sleeping worker isn't yet counted in
    /// `parked_count`, whereas a checkpoint-waiting worker is. Keeping them
    /// distinct lets `park_workers()` wake idlers (body_sleep_cv) without
    /// disturbing the parked-count barrier, and lets `unpark_workers()`
    /// release parked workers (resume_cv) without waking idlers.
    #[cfg(feature = "test_time")]
    body_sleep_mu: Mutex<()>,
    #[cfg(feature = "test_time")]
    body_sleep_cv: parking_lot::Condvar,
}

impl ParkControl {
    fn new() -> Self {
        Self {
            park: AtomicBool::new(false),
            parked_count: Mutex::new(0),
            parked_cv: parking_lot::Condvar::new(),
            resume_cv: parking_lot::Condvar::new(),
            resumed_cv: parking_lot::Condvar::new(),
            #[cfg(feature = "test_time")]
            body_sleep_mu: Mutex::new(()),
            #[cfg(feature = "test_time")]
            body_sleep_cv: parking_lot::Condvar::new(),
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
        #[cfg(feature = "test_time")]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
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
        // In test_time mode: wake body-sleeping workers via condvar so they
        // loop back to the checkpoint and park.
        //
        // Hold body_sleep_mu while notifying to prevent the classic lost-wakeup
        // race: without the lock, notify_all() can fire AFTER a worker checks
        // the condvar condition (finds park=false, enters wait) but BEFORE it
        // calls wait() — the signal is lost and the worker sleeps forever.
        // Holding the lock ensures either the worker sees park=true (set above,
        // SeqCst) and skips wait(), or the worker is already inside wait() and
        // will be woken. (parking_lot condvar wait releases the lock atomically.)
        //
        // In production (non-test_time): workers poll `park` between their short
        // thread::sleep()s, so they reach the checkpoint within one sleep
        // interval — no explicit wakeup needed (and thread::unpark() would not
        // wake a thread::sleep() anyway).
        #[cfg(feature = "test_time")]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
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
        // Wake body-sleeping workers (test_time mode) so they observe shutdown.
        // Hold the lock to prevent lost-wakeup (see park_workers comment).
        #[cfg(feature = "test_time")]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
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
// `recv_slot` / `out_tx` are only touched on the non-test_time receive
// path; in test_time the recv worker is a pure parking service.
#[cfg_attr(feature = "test_time", allow(unused_variables))]
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
        #[cfg(feature = "test_time")]
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

        #[cfg(not(feature = "test_time"))]
        {
        // Don't start a receive() if park was requested.
        if park.park.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_micros(100));
            continue;
        }

        let mut recv = match recv_slot.try_lock().and_then(|mut g| g.take()) {
            Some(h) => h,
            None => {
                thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }
        };

        let output = recv.receive();

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
        if !shutdown.load(Ordering::SeqCst) && !park.park.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_micros(100));
        }
        } // end #[cfg(not(feature = "test_time"))]
    }
}

/// Send worker loop. Symmetric to [`recv_worker_loop`]: the
/// `SendHandle` lives in the shared `send_slot` so a parked-window Sim
/// system can borrow it via [`SendHandleRes`] for cross-half work that
/// needs the send half. Same per-iteration deposit discipline keeps the
/// park-window borrow race-free.
///
/// In `test_time` this worker is a **pure parking service** (like the recv
/// worker): the consumer drives the send synchronously inside its park
/// window, so `snap_rx` / `send_slot` are unused here.
#[cfg_attr(feature = "test_time", allow(unused_variables))]
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

        // In test_time the send worker is a PURE PARKING SERVICE: the consumer
        // drives the send (preamble + scope changes + send_all_packets)
        // synchronously inside its park window, so handshake-response and
        // snapshot delivery occur at a deterministic point each tick rather than
        // whenever this real-time thread happens to be scheduled. That real-time
        // timing was load-dependent and could reorder connect handshakes under
        // parallel test load, perturbing avatar spawn order (rapier handle
        // assignment) → non-deterministic physics. Body-sleep until the next
        // park (zero CPU); never touch the snapshot or the send handle.
        #[cfg(feature = "test_time")]
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

        #[cfg(not(feature = "test_time"))]
        {
            // Don't start send work if park was requested — send_all_packets
            // holds time_manager.read() for its entire packet loop, which can
            // block the recv worker's time_manager.write() (in take_tick_events);
            // skipping lets this worker reach its checkpoint without contention.
            if park.park.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }

            // Claim the handle via try_lock (never block the park checkpoint).
            let snap_opt = snap_rx.take_latest();
            let mut send = match send_slot.try_lock().and_then(|mut g| g.take()) {
                Some(h) => h,
                None => {
                    thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                }
            };

            // Flush handshake responses every iteration so the initial
            // handshake completes before Sim generates its first snapshot.
            send.apply_pending_send_preamble();

            if let Some(snap) = snap_opt {
                send.apply_pending_scope_changes(&snap);
                send.send_all_packets(snap);
            } else {
                loop {
                    match send_slot.try_lock() {
                        Some(mut g) => { *g = Some(send); break; }
                        None => { thread::sleep(std::time::Duration::from_micros(100)); }
                    }
                }
                thread::sleep(std::time::Duration::from_micros(100));
                continue;
            }

            // Re-deposit before looping back to the park checkpoint.
            loop {
                match send_slot.try_lock() {
                    Some(mut g) => { *g = Some(send); break; }
                    None => { thread::sleep(std::time::Duration::from_micros(100)); }
                }
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
    let fresh_output = recv.receive();
    outputs.push(fresh_output);

    if outputs.iter().all(|o| o.is_empty()) {
        world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
        *recv_slot.lock() = Some(recv);
        *send_slot.lock() = Some(send);
        return;
    }

    // Cross-half receive orchestration per ReceiveOutput.
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

    // Return the handles (no unpark — caller's responsibility).
    world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
    *recv_slot.lock() = Some(recv);
    *send_slot.lock() = Some(send);
}

/// Bevy system installed on the consumer Sim app. Drains
/// `ReceiveOutput<Entity>` from the Recv worker's bounded channel, runs
/// the cross-half receive orchestration against the Sim world, and fans
/// the resulting events into the bevy `Messages<X>` buffers + the
/// `SimEventReceiver`.
///
/// Runs in `Update` by default (same as `process_packets` for the
/// non-`sim_integration_full` variants).
///
/// In `test_time` mode this system is a no-op: the recv worker does not
/// call `recv.receive()` (it sleeps as a pure parking service), and all
/// recv work is done synchronously inside `drain_recv_impl`, called from
/// the consumer's per-tick park window (e.g. cyberlith's `open_park_window`).
/// This eliminates the double park/unpark cycle (once here, once in
/// `open_park_window`) that caused a hard-to-reproduce race condition.
pub fn drain_recv_worker_output(world: &mut World) {
    // In test_time mode: the recv worker is a pure parking service and
    // recv.receive() is called synchronously from the consumer's park
    // window. Skip the park+recv+unpark cycle here entirely.
    #[cfg(feature = "test_time")]
    {
        let _ = world;
        return;
    }

    #[cfg(not(feature = "test_time"))]
    {
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

        // Park the workers first so any in-flight recv.receive() iteration
        // completes and the recv handle is deposited back in the slot before
        // we proceed.
        let (recv_slot, send_slot) = {
            let state = world.resource::<PluginInternalState>();
            state.park_workers();
            (
                Arc::clone(&world.resource::<RecvHandleRes>().0),
                Arc::clone(&world.resource::<SendHandleRes>().0),
            )
        };

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
            world.resource::<PluginInternalState>().unpark_workers();
            return;
        }

        let mut sim_handle = sim_handle_opt.unwrap();
        let mut recv = recv_opt.unwrap();
        let mut send = send_opt.unwrap();

        let mut outputs: Vec<ReceiveOutput<Entity>> = receiver.try_iter().collect();
        let fresh_output = recv.receive();
        outputs.push(fresh_output);

        if outputs.iter().all(|o| o.is_empty()) {
            world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
            *recv_slot.lock() = Some(recv);
            *send_slot.lock() = Some(send);
            world.resource::<PluginInternalState>().unpark_workers();
            return;
        }

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

        world.resource_mut::<SimHandleRes>().0 = Some(sim_handle);
        *recv_slot.lock() = Some(recv);
        *send_slot.lock() = Some(send);
        world.resource::<PluginInternalState>().unpark_workers();
    }
}

/// Bevy system that surfaces any worker panic onto the main thread.
/// Installed in the same default schedule as `drain_recv_worker_output`.
pub fn propagate_worker_panics(state: bevy_ecs::system::Res<PluginInternalState>) {
    state.propagate_panic_if_any();
}
