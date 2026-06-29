//! MISSION_PIPELINE_API_BOUNDARY G7-3 — the framework-agnostic pipeline worker
//! runtime, moved out of the bevy adapter's `plugin_full.rs` into naia-server
//! core so ANY consumer (bevy or non-bevy) gets turnkey pipelining.
//!
//! [`PipelineRuntime<E>`] owns the Recv + Send worker threads, the parked-count
//! park/unpark barrier, the per-iteration park-window borrow discipline, the
//! `ReceiveOutput` hand-off channel, the snapshot lag buffer wiring, and the
//! Armed→Running→Stopped lifecycle. It uses only `std::thread` + channels +
//! `parking_lot`/`smol` primitives — NO framework types. The bevy adapter keeps
//! ONLY the bevy-coupled pieces: the `ReceiveOutput` event fan-out into bevy
//! `Messages<X>` (`drain_recv_impl*`), the bevy `Resource` wrappers, and system
//! registration. It constructs a `PipelineRuntime<Entity>` and delegates
//! `listen`/`park_workers`/`unpark_workers`/`propagate_panic_if_any` to it.
//!
//! ## Parked vs active (the `workers_active` cfg)
//!
//! `workers_active = not(deterministic)` (emitted by `build.rs`). The two modes:
//!   - **not(workers_active)** (deterministic test/determinism harness): the
//!     workers are PURE PARKING SERVICES — the consumer drives recv/send
//!     synchronously inside its park window, so handshake responses + snapshot
//!     delivery land at a deterministic point each tick. The workers only
//!     body-sleep until parked. This is the byte-exact determinism path.
//!   - **workers_active** (production / bench): the workers actively drain the
//!     socket (recv) and transmit the lagged frozen send job (send) between park
//!     windows, overlapping the consumer's gameplay tick.

use std::{
    any::Any,
    hash::Hash,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

#[cfg(workers_active)]
use naia_shared::SnapshotWorld;

use crate::pipeline_actors::SnapshotReceiver;
use crate::transport::PacketReadiness;
use crate::{ReceiveOutput, RecvHandle, SendHandle};

// ─── Timing hooks ────────────────────────────────────────────────────────────

/// Optional per-stage timing recorders the consumer can wire to its own
/// aggregator (e.g. the bevy adapter's bench-only `pipeline_timing` module).
/// Each is an `Option<fn(u64)>` taking elapsed nanoseconds. `None` ⇒ the stage
/// is not even measured (zero overhead). This keeps the core runtime free of any
/// adapter-specific instrumentation crate while preserving the bench profile.
#[derive(Clone, Copy, Default)]
pub struct RuntimeTimingHooks {
    /// Socket `recv.receive()` cost in the recv worker.
    pub record_recv: Option<fn(u64)>,
    /// `transmit_send_job` / `send_all_packets` cost in the send worker.
    pub record_send: Option<fn(u64)>,
    /// `park_workers()` barrier wait (main thread blocked for the checkpoint).
    pub record_barrier: Option<fn(u64)>,
}

// ─── Lifecycle state ─────────────────────────────────────────────────────────

/// Lifecycle of a [`PipelineRuntime`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RuntimeState {
    /// Constructed; handles parked in their slots; workers not yet spawned.
    Armed = 0,
    /// `spawn_workers` ran; both worker threads live.
    Running = 1,
    /// Dropped / shut down.
    Stopped = 2,
}

// ─── ParkControl ─────────────────────────────────────────────────────────────

/// Park-control flags + condvar pair (parking_lot Mutex + Condvar) used to
/// coordinate the main thread parking the worker threads at their checkpoints.
struct ParkControl {
    /// `true` ⇒ workers should park at the top of their loop iteration.
    park: AtomicBool,
    /// Number of workers currently parked. Main waits until this hits the
    /// worker count in [`PipelineRuntime::park_workers`].
    parked_count: Mutex<u32>,
    /// Signalled by a worker when it increments `parked_count` at the
    /// checkpoint. `park_workers()` waits on this.
    parked_cv: parking_lot::Condvar,
    /// Signalled by `unpark_workers()` (under the `parked_count` lock) when it
    /// clears `park`. Parked workers wait on this to leave the checkpoint.
    /// Replaces a prior `thread::park()`/`unpark()` scheme that had a token
    /// race: a worker that consumed its unpark token but was descheduled before
    /// re-reading `park` would re-read the *next* cycle's `park=true` and
    /// re-block with no token, never re-incrementing `parked_count` → deadlock.
    /// A condvar guarded by the same mutex as the `park` read has no such lost
    /// transition.
    resume_cv: parking_lot::Condvar,
    /// Signalled by a worker when it decrements `parked_count` on leaving the
    /// checkpoint. `unpark_workers()` waits on this until `parked_count` is back
    /// to 0 — i.e. it is **synchronous**: it does not return until every parked
    /// worker has observed `park=false` and left the checkpoint. Because
    /// `unpark_workers()` and the next `park_workers()` run sequentially on the
    /// same thread, this guarantees `park` can never be re-set to `true` while a
    /// worker is still mid-resume.
    resumed_cv: parking_lot::Condvar,
    /// Mutex + condvar for workers to sleep between park windows. Used by BOTH
    /// paths (MISSION_OVERLAP_FRONTIER T1):
    ///   - not(workers_active): the worker body-sleeps on an UNBOUNDED `wait()`
    ///     until woken (it has no other work).
    ///   - workers_active: the worker idle-waits on a BOUNDED `wait_for(100µs)`
    ///     between iterations; signalling here wakes it instantly instead of
    ///     after up to one ~100µs poll (the park-barrier win).
    /// Separate from the *checkpoint* wait (`resume_cv`): a body-sleeping worker
    /// isn't yet counted in `parked_count`, whereas a checkpoint-waiting worker
    /// is.
    body_sleep_mu: Mutex<()>,
    body_sleep_cv: parking_lot::Condvar,
    /// Event-driven control wake (workers_active): an awaitable, coalescing
    /// `bounded(1)` signal the recv worker selects on alongside the transport's
    /// packet-readiness. Every site that must wake an idle worker —
    /// `park_workers`, `Drop` (shutdown), test-panic — pings it via
    /// [`Self::ping_control`]. The `not(workers_active)` path uses
    /// `body_sleep_cv` and ignores this.
    control_tx: smol::channel::Sender<()>,
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
    /// drop-if-closed are both fine — a buffered token already guarantees the
    /// wake, and a closed channel means the worker is gone).
    fn ping_control(&self) {
        let _ = self.control_tx.try_send(());
    }
}

/// Captured panic from a worker thread. Surfaced on main via
/// [`PipelineRuntime::propagate_panic_if_any`].
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

struct WorkerHandle {
    name: &'static str,
    join: Option<JoinHandle<()>>,
}

// ─── PipelineRuntime ─────────────────────────────────────────────────────────

/// The framework-agnostic Recv + Send worker runtime. See module docs.
///
/// On drop, signals shutdown to the worker threads and blocks until they join
/// (with a 5s soft-deadline before logging a warning + leaking the thread).
pub struct PipelineRuntime<E: Copy + Eq + Hash + Send + Sync + 'static> {
    state: AtomicU8,
    /// Channel for the Recv worker to push `ReceiveOutput` to the consumer.
    recv_out_chan_rx: Mutex<Option<Receiver<ReceiveOutput<E>>>>,
    /// Sender half retained so it can be dropped on shutdown (signals the
    /// consumer-side drain that the worker is gone).
    recv_out_chan_tx: Mutex<Option<Sender<ReceiveOutput<E>>>>,
    /// SnapshotReceiver held until the Send worker is spawned.
    snapshot_receiver: Mutex<Option<SnapshotReceiver<E>>>,
    /// Shutdown signaller: dropping flips `true`, observed by workers at the top
    /// of each loop iteration.
    shutdown: Arc<AtomicBool>,
    /// Park/unpark coordination.
    park: Arc<ParkControl>,
    /// Captured panic payload (first worker to panic wins).
    panic_slot: Arc<PanicSlot>,
    /// Worker thread handles. Empty until `spawn_workers`; drained on Drop.
    workers: Mutex<Vec<WorkerHandle>>,
    /// Park-window slot for the recv worker's [`RecvHandle`] — the same `Arc`
    /// held by the [`crate::pipeline_actors::PipelinedServer`]'s pipeline, so a
    /// parked-window consumer system can take/return the handle.
    recv_slot: Arc<Mutex<Option<RecvHandle<E>>>>,
    /// Park-window slot for the send worker's [`SendHandle`] — symmetric.
    send_slot: Arc<Mutex<Option<SendHandle<E>>>>,
    /// Optional per-stage timing recorders (see [`RuntimeTimingHooks`]).
    timing: RuntimeTimingHooks,
    /// Test-only: when set, workers panic on their next loop iteration.
    #[cfg(any(test, feature = "test_time"))]
    test_panic_request: Arc<AtomicBool>,
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> PipelineRuntime<E> {
    /// Construct an **armed** runtime around the pipeline's shared handle slots
    /// and the snapshot lag channel's receiver. Workers are NOT spawned yet —
    /// call [`Self::spawn_workers`] after the socket is bound.
    ///
    /// `recv_slot` / `send_slot` MUST be the same `Arc`s the
    /// [`crate::pipeline_actors::PipelinedServer`] holds (via its
    /// `recv_slot()` / `send_slot()` accessors), so the workers and the
    /// consumer's park-window borrow share one underlying `Mutex<Option<…>>`.
    pub fn new_armed(
        recv_slot: Arc<Mutex<Option<RecvHandle<E>>>>,
        send_slot: Arc<Mutex<Option<SendHandle<E>>>>,
        snapshot_receiver: SnapshotReceiver<E>,
        timing: RuntimeTimingHooks,
    ) -> Self {
        // UNBOUNDED: the recv worker calls `recv.receive()` (which destructively
        // drains command packets off the socket into a `ReceiveOutput`) once per
        // wakeup, but the consumer drains this channel only once per park window
        // (≈ once per service tick). When packets arrive in a burst the worker
        // produces several outputs between drains. A bounded(1) channel silently
        // `drop()`ed every output past the first — and because those outputs
        // already held packets pulled OFF the socket, the drain's safety-net
        // `recv.receive()` could not recover them (measured ~93–98% command loss
        // under sustained input). Unbounded never drops; the drain's
        // `try_iter().collect()` empties it FIFO each cycle, preserving ordering
        // and determinism. Memory is self-limiting: at most one drain-interval's
        // worth accumulates.
        let (tx, rx) = unbounded::<ReceiveOutput<E>>();
        Self {
            state: AtomicU8::new(RuntimeState::Armed as u8),
            recv_out_chan_rx: Mutex::new(Some(rx)),
            recv_out_chan_tx: Mutex::new(Some(tx)),
            snapshot_receiver: Mutex::new(Some(snapshot_receiver)),
            shutdown: Arc::new(AtomicBool::new(false)),
            park: Arc::new(ParkControl::new()),
            panic_slot: Arc::new(PanicSlot::new()),
            workers: Mutex::new(Vec::new()),
            recv_slot,
            send_slot,
            timing,
            #[cfg(any(test, feature = "test_time"))]
            test_panic_request: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A clone of the recv-output channel's receiver, for the consumer-side
    /// drain. `None` if the runtime has been shut down. crossbeam `Receiver`s
    /// are clonable; all clones observe the same FIFO queue.
    pub fn recv_out_receiver(&self) -> Option<Receiver<ReceiveOutput<E>>> {
        self.recv_out_chan_rx.lock().as_ref().cloned()
    }

    /// Current lifecycle state.
    pub fn state(&self) -> RuntimeState {
        match self.state.load(Ordering::Acquire) {
            0 => RuntimeState::Armed,
            1 => RuntimeState::Running,
            _ => RuntimeState::Stopped,
        }
    }

    /// Test/dev hook: request that workers panic on their next loop iteration.
    #[cfg(any(test, feature = "test_time"))]
    pub fn request_worker_panic_for_test(&self) {
        self.test_panic_request.store(true, Ordering::SeqCst);
        // Wake any parked or idle (body-sleeping) workers so they loop back and
        // observe the request.
        self.park.resume_cv.notify_all();
        #[cfg(not(workers_active))]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Wake the event-driven (workers_active) worker too.
        self.park.ping_control();
    }

    /// Spawn the Recv + Send worker threads and transition `Armed → Running`.
    ///
    /// `recv_readiness` is the transport's event-driven readiness (`Some` for
    /// the in-process `PacketChannel`; `None` for poll-only sockets); the recv
    /// worker selects on it instead of polling. The caller MUST have already
    /// bound the socket (so the recv handle's `readiness()` is meaningful) and
    /// re-deposited the handles into their slots before calling this.
    ///
    /// Panics if called in a non-`Armed` state.
    pub fn spawn_workers(&self, recv_readiness: Option<PacketReadiness>) {
        assert_eq!(
            self.state(),
            RuntimeState::Armed,
            "PipelineRuntime::spawn_workers called in non-Armed state",
        );

        let recv_tx = self
            .recv_out_chan_tx
            .lock()
            .clone()
            .expect("recv_out_chan_tx Some in Armed state");

        let recv_slot = Arc::clone(&self.recv_slot);
        let shutdown_recv = Arc::clone(&self.shutdown);
        let park_recv = Arc::clone(&self.park);
        let panic_recv = Arc::clone(&self.panic_slot);
        let timing_recv = self.timing;
        #[cfg(any(test, feature = "test_time"))]
        let test_panic_recv = Arc::clone(&self.test_panic_request);

        // A single shared clock Arc for ALL workers: `TestClock::advance` on the
        // calling (consumer/test) thread must advance the same clock both
        // workers read. Two separate `shareable_handle()` calls would create two
        // distinct Arcs — the calling thread would only be linked to the last.
        #[cfg(feature = "test_time")]
        let clock_handle_shared = naia_shared::TestClock::shareable_handle();
        #[cfg(feature = "test_time")]
        let clock_handle_recv = Arc::clone(&clock_handle_shared);

        let recv_join = thread::Builder::new()
            .name("naia-recv-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_shared::TestClock::install_shared(clock_handle_recv);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recv_worker_loop(
                        &recv_slot,
                        &recv_tx,
                        &shutdown_recv,
                        &park_recv,
                        recv_readiness,
                        timing_recv,
                        #[cfg(any(test, feature = "test_time"))]
                        &test_panic_recv,
                    );
                }));

                if let Err(payload) = result {
                    panic_recv.set(payload);
                }

                #[cfg(feature = "test_time")]
                naia_shared::TestClock::detach_shared();
            })
            .expect("spawn recv worker thread");

        let snap_rx = self
            .snapshot_receiver
            .lock()
            .take()
            .expect("snapshot_receiver Some in Armed state");

        let send_slot = Arc::clone(&self.send_slot);
        let shutdown_send = Arc::clone(&self.shutdown);
        let park_send = Arc::clone(&self.park);
        let panic_send = Arc::clone(&self.panic_slot);
        let timing_send = self.timing;
        #[cfg(any(test, feature = "test_time"))]
        let test_panic_send = Arc::clone(&self.test_panic_request);

        #[cfg(feature = "test_time")]
        let clock_handle_send = Arc::clone(&clock_handle_shared);

        let send_join = thread::Builder::new()
            .name("naia-send-worker".into())
            .spawn(move || {
                #[cfg(feature = "test_time")]
                naia_shared::TestClock::install_shared(clock_handle_send);

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    send_worker_loop(
                        &send_slot,
                        &snap_rx,
                        &shutdown_send,
                        &park_send,
                        timing_send,
                        #[cfg(any(test, feature = "test_time"))]
                        &test_panic_send,
                    );
                }));

                if let Err(payload) = result {
                    panic_send.set(payload);
                }

                #[cfg(feature = "test_time")]
                naia_shared::TestClock::detach_shared();
            })
            .expect("spawn send worker thread");

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

        self.state
            .store(RuntimeState::Running as u8, Ordering::Release);
    }

    /// Park both worker threads (block until both reach the top of their idle
    /// loop). After return, callers may safely `TestClock::advance(...)` or
    /// borrow handles from the slots without racing the workers.
    ///
    /// No-op when in `Armed` (workers not yet spawned) or `Stopped`.
    ///
    /// # Panic / exit safety
    /// A worker that has panicked or exited never reaches its checkpoint, so a
    /// naive condvar wait would deadlock. The wait breaks early once every
    /// not-yet-parked worker thread has *finished* — the live workers are all
    /// parked and the rest are gone. A subsequent [`Self::propagate_panic_if_any`]
    /// surfaces the captured payload.
    pub fn park_workers(&self) {
        if self.state() != RuntimeState::Running {
            return;
        }
        let expected = self.workers.lock().len() as u32;
        self.park.park.store(true, Ordering::SeqCst);
        // Wake body-sleeping workers via condvar so they loop back to the
        // checkpoint and park immediately. Hold body_sleep_mu while notifying to
        // prevent the lost-wakeup race: without the lock, notify_all() can fire
        // AFTER a worker checks the condition (park=false) but BEFORE it calls
        // wait() — the signal is lost. Holding the lock (which the worker also
        // holds across its park-check + wait) ensures either the worker sees
        // park=true (set above, SeqCst) and skips the wait, or it is already
        // inside wait() and is woken.
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Event-driven (workers_active) path: wake the active recv worker
        // blocked in its `future::or` select.
        self.park.ping_control();

        let _t_barrier = self.timing.record_barrier.map(|_| std::time::Instant::now());
        let mut g = self.park.parked_count.lock();
        while *g < expected {
            // Break early if the not-yet-parked workers have all finished
            // (panicked/exited) — they will never park.
            let finished = self
                .workers
                .lock()
                .iter()
                .filter(|w| w.join.as_ref().map(|j| j.is_finished()).unwrap_or(true))
                .count() as u32;
            if *g + finished >= expected {
                break;
            }
            // Bounded wait so a worker that finishes after the check above (but
            // before we re-loop) is still observed promptly.
            self.park
                .parked_cv
                .wait_for(&mut g, Duration::from_millis(5));
        }
        if let (Some(f), Some(t)) = (self.timing.record_barrier, _t_barrier) {
            f(t.elapsed().as_nanos() as u64);
        }
    }

    /// Resume both worker threads. **Synchronous**: does not return until every
    /// parked worker has observed `park=false` and left the checkpoint
    /// (`parked_count` back to 0). This is what makes the next `park_workers()`
    /// safe — it cannot set `park=true` while a worker is still mid-resume.
    pub fn unpark_workers(&self) {
        if self.state() != RuntimeState::Running {
            return;
        }
        let mut g = self.park.parked_count.lock();
        // Clear the park flag + wake checkpoint waiters under the same lock the
        // checkpoint reads `park` under (no lost wakeup).
        self.park.park.store(false, Ordering::SeqCst);
        self.park.resume_cv.notify_all();
        while *g > 0 {
            let finished_unparked = self
                .workers
                .lock()
                .iter()
                .filter(|w| w.join.as_ref().map(|j| j.is_finished()).unwrap_or(true))
                .count() as u32;
            // If the only thing keeping the count above 0 would be a worker that
            // has since finished, stop waiting.
            if finished_unparked >= *g {
                break;
            }
            self.park
                .resumed_cv
                .wait_for(&mut g, Duration::from_millis(5));
        }
    }

    /// If a worker thread has panicked, re-panic on the calling thread
    /// (surfacing the captured payload). Call once per consumer tick or before
    /// any test assertion.
    pub fn propagate_panic_if_any(&self) {
        if let Some(payload) = self.panic_slot.take() {
            std::panic::resume_unwind(payload);
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync + 'static> Drop for PipelineRuntime<E> {
    fn drop(&mut self) {
        // Signal shutdown.
        self.shutdown.store(true, Ordering::SeqCst);
        // Drop the recv channel sender so the consumer-side drain stops
        // expecting more outputs.
        self.recv_out_chan_tx.lock().take();
        // Clear the park flag + wake checkpoint waiters so any parked worker
        // leaves the checkpoint and observes shutdown. Under the parked_count
        // lock (same lock the checkpoint reads `park` under) to avoid a lost
        // wakeup.
        {
            let _g = self.park.parked_count.lock();
            self.park.park.store(false, Ordering::SeqCst);
            self.park.resume_cv.notify_all();
        }
        // Wake body-sleeping workers (parked mode) so they observe shutdown.
        #[cfg(not(workers_active))]
        {
            let _g = self.park.body_sleep_mu.lock();
            self.park.body_sleep_cv.notify_all();
        }
        // Wake the event-driven (workers_active) recv worker so it leaves its
        // `future::or` and observes shutdown at the loop top.
        self.park.ping_control();

        let mut workers = std::mem::take(&mut *self.workers.lock());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        for w in workers.iter_mut() {
            let name = w.name;
            if let Some(join) = w.join.take() {
                while std::time::Instant::now() < deadline {
                    if join.is_finished() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                if join.is_finished() {
                    if join.join().is_err() {
                        log::warn!("naia pipeline worker {name} panicked during shutdown");
                    }
                } else {
                    log::warn!("naia pipeline worker {name} did not exit within 5s; leaking thread");
                }
            }
        }
        self.state
            .store(RuntimeState::Stopped as u8, Ordering::Release);
    }
}

// ─── Worker loops ────────────────────────────────────────────────────────────

/// Top-of-iteration: if `park` is set, increment `parked_count`, notify main,
/// then wait on `resume_cv` until `unpark_workers()` clears `park`.
fn worker_park_checkpoint(park: &ParkControl) {
    // Fast path: not parking, no lock needed.
    if !park.park.load(Ordering::SeqCst) {
        return;
    }
    let mut g = park.parked_count.lock();
    // Re-check under the lock: `park` may have been cleared between the unlocked
    // load above and acquiring the lock.
    if !park.park.load(Ordering::SeqCst) {
        return;
    }
    *g += 1;
    park.parked_cv.notify_all();
    // Park loop: wait on the resume condvar until `park` clears. The condvar
    // wait atomically releases `g`; the `park` read happens under `g`, and
    // `unpark_workers()` clears `park` + notifies under the same lock — so there
    // is no lost transition and no token to drop.
    while park.park.load(Ordering::SeqCst) {
        park.resume_cv.wait(&mut g);
    }
    // Leaving the checkpoint: decrement and signal `unpark_workers()`.
    *g -= 1;
    park.resumed_cv.notify_all();
}

/// Recv worker loop. The `RecvHandle` is **not** owned by this closure — it
/// lives in the shared `recv_slot` (`Arc<Mutex<Option<…>>>`) so a parked-window
/// consumer system can borrow it for the per-user tick-buffer drain.
///
/// Per-iteration discipline that keeps the park-window borrow race-free:
/// 1. **Park checkpoint with the handle deposited** — the handle sits in
///    `recv_slot` whenever the worker is at the checkpoint, so once
///    [`PipelineRuntime::park_workers`] returns, the consumer is guaranteed to
///    find the handle in the slot.
/// 2. **Claim** the handle for the brief receive window.
/// 3. `recv.receive()` + ship the `ReceiveOutput`.
/// 4. **Deposit** the handle back *before* looping to the checkpoint.
#[cfg_attr(not(workers_active), allow(unused_variables))]
fn recv_worker_loop<E: Copy + Eq + Hash + Send + Sync + 'static>(
    recv_slot: &Arc<Mutex<Option<RecvHandle<E>>>>,
    out_tx: &Sender<ReceiveOutput<E>>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    readiness: Option<PacketReadiness>,
    timing: RuntimeTimingHooks,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    loop {
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(any(test, feature = "test_time"))]
        if test_panic.load(Ordering::SeqCst) {
            panic!("test-requested recv worker panic");
        }

        // In not(workers_active) (deterministic) mode the consumer drives all
        // `recv.receive()` calls synchronously while workers are parked. Running
        // receive() here concurrently would cause RwLock contention on
        // time_manager and could deadlock park_workers(). So the parked recv
        // worker is a pure parking service: body-sleep until the next park.
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
            // Don't start a receive() if park was requested — loop straight back
            // to the checkpoint (which parks immediately). No sleep: a sleep
            // would just add up to 100µs to the barrier in the race window where
            // park is set after the checkpoint returned. (T1)
            if park.park.load(Ordering::SeqCst) {
                continue;
            }

            // Claim with `try_lock` so the worker always loops back to the park
            // checkpoint rather than blocking. A blocking `.lock()` could leave
            // the worker stuck while `park_workers()` waits for it to reach its
            // checkpoint — a deadlock. A contended slot just produces a 100µs
            // retry.
            let mut recv = match recv_slot.try_lock().and_then(|mut g| g.take()) {
                Some(h) => h,
                None => {
                    thread::sleep(Duration::from_micros(100));
                    continue;
                }
            };

            let _t_recv = timing.record_recv.map(|_| std::time::Instant::now());
            let output = recv.receive();
            if let (Some(f), Some(t)) = (timing.record_recv, _t_recv) {
                f(t.elapsed().as_nanos() as u64);
            }

            // Re-deposit using try_lock spin (never blocks park checkpoint).
            loop {
                match recv_slot.try_lock() {
                    Some(mut g) => {
                        *g = Some(recv);
                        break;
                    }
                    None => {
                        thread::sleep(Duration::from_micros(100));
                    }
                }
            }

            // Unbounded channel: NEVER drop a `ReceiveOutput` — it already holds
            // command packets pulled off the socket. `send` only errors if the
            // receiver hung up (teardown) — exit cleanly then.
            if out_tx.send(output).is_err() {
                return;
            }
            // Idle inter-iteration wait.
            match &readiness {
                // Event-driven (in-process PacketChannel): block with ZERO CPU
                // until either the transport signals a packet may be ready OR a
                // control wake (park / shutdown / test-panic) fires. Eliminates
                // the 100µs idle-poll storm.
                //
                // Lost-wakeup-free: async-channel `recv()` re-checks the buffer
                // before parking, and both signals are bounded(1) coalescing — a
                // ping that lands between this worker's drain and its next wait
                // leaves a buffered token, so the freshly-created future resolves
                // immediately. The per-tick park-window drain remains the
                // authoritative safety net (≤1-tick dwell ceiling).
                Some(readiness) => {
                    if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                        smol::block_on(smol::future::or(readiness.wait(), async {
                            let _ = park.control_rx.recv().await;
                        }));
                    }
                    // Clear the token(s) that woke us so the next wait starts
                    // fresh (data is drained by recv.receive() at the loop top).
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
                            .wait_for(&mut g, Duration::from_micros(100));
                    }
                    drop(g);
                }
            }
        } // end #[cfg(workers_active)]
    }
}

/// Send worker loop. Symmetric to [`recv_worker_loop`]: the `SendHandle` lives
/// in the shared `send_slot` so a parked-window consumer system can borrow it.
///
/// In not(workers_active) (deterministic) mode this worker is a **pure parking
/// service**: the consumer drives the send synchronously inside its park window,
/// so `snap_rx` / `send_slot` are unused here.
#[cfg_attr(not(workers_active), allow(unused_variables))]
fn send_worker_loop<E: Copy + Eq + Hash + Send + Sync + 'static>(
    send_slot: &Arc<Mutex<Option<SendHandle<E>>>>,
    snap_rx: &SnapshotReceiver<E>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    timing: RuntimeTimingHooks,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    // MISSION_TICK_FLOOR Lever 3: one-tick send lag. The job published this
    // cycle is buffered here and transmitted on the NEXT cycle, so the worker
    // sends the previous tick's frozen job. The frozen rider prevents a torn
    // read of the live `global_dirty`.
    #[cfg(workers_active)]
    let mut held_job: Option<SnapshotWorld<E>> = None;
    loop {
        worker_park_checkpoint(park);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(any(test, feature = "test_time"))]
        if test_panic.load(Ordering::SeqCst) {
            panic!("test-requested send worker panic");
        }

        // not(workers_active): pure parking service (see fn docs). The consumer
        // drives the send synchronously inside its park window so handshake +
        // snapshot delivery land at a deterministic point each tick. Body-sleep
        // until the next park (zero CPU); never touch the snapshot or handle.
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
            // holds time_manager.read() for its entire loop, which can block the
            // recv worker's time_manager.write(); skipping lets this worker reach
            // its checkpoint without contention. (T1)
            if park.park.load(Ordering::SeqCst) {
                continue;
            }

            // Claim the handle FIRST — before touching the lag buffer — so a
            // failed claim cannot drop a buffered job. try_lock never blocks the
            // park checkpoint.
            let mut send = match send_slot.try_lock().and_then(|mut g| g.take()) {
                Some(h) => h,
                None => {
                    thread::sleep(Duration::from_micros(100));
                    continue;
                }
            };

            // One-tick lag: buffer this cycle's freshly-published job; transmit
            // the job buffered on the PREVIOUS cycle. `replace` returns the prior
            // buffered job (to send now); when no new job was published, flush
            // whatever is still buffered.
            let job_to_send = match snap_rx.take_latest() {
                Some(new_job) => held_job.replace(new_job),
                None => held_job.take(),
            };

            let _t_send = timing.record_send.map(|_| std::time::Instant::now());

            if let Some(mut job) = job_to_send {
                // Dispatch on the prepared send plan. An active job carries a
                // `SendPlan` built on MAIN at the freeze point (frozen DiffMasks
                // + frozen dirty domain + live masks already cleared), so this
                // lagged transmit reads ZERO live per-user diff state. A job
                // without a plan (not expected on this path) falls back to a
                // synchronous prepare+transmit.
                let _ = job.take_frozen_dirty();
                match job.take_send_plan() {
                    Some(plan) => {
                        // Drain the ACK channel in the worker preamble (before
                        // transmit) so `sent_updates` is consumed on the send
                        // side — single-owner. The no-plan fallback below drains
                        // inside `send_all_packets`.
                        send.drain_all_acks();
                        send.transmit_send_job(job, plan);
                    }
                    None => send.send_all_packets(job),
                }
                if let (Some(f), Some(t)) = (timing.record_send, _t_send) {
                    f(t.elapsed().as_nanos() as u64);
                }

                // Re-deposit before looping back to the park checkpoint.
                loop {
                    match send_slot.try_lock() {
                        Some(mut g) => {
                            *g = Some(send);
                            break;
                        }
                        None => {
                            thread::sleep(Duration::from_micros(100));
                        }
                    }
                }
            } else {
                // Nothing buffered yet (one-tick warmup) or nothing to send:
                // re-deposit and bounded idle-wait so park_workers() wakes us
                // instantly; the 100µs bound re-polls for the next job. (T1)
                loop {
                    match send_slot.try_lock() {
                        Some(mut g) => {
                            *g = Some(send);
                            break;
                        }
                        None => {
                            thread::sleep(Duration::from_micros(100));
                        }
                    }
                }
                {
                    let mut g = park.body_sleep_mu.lock();
                    if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                        park.body_sleep_cv
                            .wait_for(&mut g, Duration::from_micros(100));
                    }
                    drop(g);
                }
                continue;
            }
        }
    }
}
