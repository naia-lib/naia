//! `MISSION_PIPELINE_API_BOUNDARY` G7-3 — the framework-agnostic pipeline worker
//! runtime, moved out of the bevy adapter's `plugin_full.rs` into naia-server
//! core so ANY consumer (bevy or non-bevy) gets turnkey pipelining.
//!
//! [`PipelineRuntime<E>`] owns the Recv + Send worker threads, the parked-count
//! park/unpark barrier, the handle lease (worker owns, consumer borrows inside the park window), the
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
//!   - **`not(workers_active)`** (deterministic test/determinism harness): the
//!     workers are PURE PARKING SERVICES — the consumer drives recv/send
//!     synchronously inside its park window, so handshake responses + snapshot
//!     delivery land at a deterministic point each tick. The workers only
//!     body-sleep until parked. This is the byte-exact determinism path.
//!   - **`workers_active`** (production / bench): the workers actively drain the
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

/// Park-control flags + condvar pair (`parking_lot` Mutex + Condvar) used to
/// coordinate the main thread parking the worker threads at their checkpoints.
pub(crate) struct ParkControl {
    /// `true` ⇒ workers should park at the top of their loop iteration.
    park: AtomicBool,
    /// `true` ⇒ the send worker must, on its next iteration, drain the
    /// one-tick-lag buffer (`held_job`) plus every published-but-untransmitted
    /// snapshot straight to io BEFORE parking. A test/harness barrier
    /// (`park_workers_flushing`) that makes snapshot delivery deterministic
    /// under active workers; the send worker `swap`s it back to `false` once it
    /// has drained. Ignored on the `not(workers_active)` path (no worker
    /// threads → nothing to flush). See `park_workers_flushing`.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    flush: AtomicBool,
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
    /// The workers' one idle wake point ([`WorkerWake`]), used by BOTH paths
    /// (`MISSION_OVERLAP_FRONTIER` T1):
    ///   - `not(workers_active)`: the worker body-sleeps UNBOUNDED until woken
    ///     (it has no other work).
    ///   - `workers_active`: the send worker sleeps UNBOUNDED until a job is
    ///     published, a park is requested or shutdown begins (nydus-y2e: zero
    ///     idle wakeups); the recv worker on a poll-only transport sleeps a
    ///     BOUNDED 100µs so the socket is still drained.
    ///
    /// `park_workers`, `Drop`, the test-panic request and
    /// [`crate::pipeline_actors::SnapshotSender::send`] all notify it.
    ///
    /// Separate from the *checkpoint* wait (`resume_cv`): a body-sleeping worker
    /// isn't yet counted in `parked_count`, whereas a checkpoint-waiting worker
    /// is.
    wake: Arc<WorkerWake>,
    /// Event-driven control wake (`workers_active)`: an awaitable, coalescing
    /// `bounded(1)` signal the recv worker selects on alongside the transport's
    /// packet-readiness. Every site that must wake an idle worker —
    /// `park_workers`, `Drop` (shutdown), test-panic — pings it via
    /// [`Self::ping_control`]. The `not(workers_active)` path uses the
    /// [`WorkerWake`] and ignores this.
    control_tx: smol::channel::Sender<()>,
    #[cfg_attr(not(workers_active), allow(dead_code))]
    control_rx: smol::channel::Receiver<()>,
    /// Measure (nydus-y2e): slot-lock acquisitions by the workers' leases. The
    /// card's DONE WHEN is 2 per receive iteration to 0.
    #[cfg(any(test, feature = "test_time"))]
    pub(crate) slot_locks: std::sync::atomic::AtomicU64,
    /// Measure (nydus-y2e): send-worker wakeups that found no job, no park and
    /// no shutdown. DONE WHEN: 0 while idle.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    #[cfg(any(test, feature = "test_time"))]
    pub(crate) send_idle_wakeups: std::sync::atomic::AtomicU64,
}

impl ParkControl {
    pub(crate) fn new() -> Self {
        // bounded(1) ⇒ coalescing: at most one pending wake token.
        let (control_tx, control_rx) = smol::channel::bounded(1);
        Self {
            park: AtomicBool::new(false),
            flush: AtomicBool::new(false),
            parked_count: Mutex::new(0),
            parked_cv: parking_lot::Condvar::new(),
            resume_cv: parking_lot::Condvar::new(),
            resumed_cv: parking_lot::Condvar::new(),
            wake: Arc::new(WorkerWake::new()),
            control_tx,
            control_rx,
            #[cfg(any(test, feature = "test_time"))]
            slot_locks: std::sync::atomic::AtomicU64::new(0),
            #[cfg(any(test, feature = "test_time"))]
            send_idle_wakeups: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// The workers' idle wake point, shared with the snapshot publisher.
    pub(crate) fn wake(&self) -> Arc<WorkerWake> {
        Arc::clone(&self.wake)
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
    /// `SnapshotReceiver` held until the Send worker is spawned.
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
    /// held by the [`crate::pipeline_actors::PipelinedWorldServer`]'s pipeline, so a
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
    /// [`crate::pipeline_actors::PipelinedWorldServer`] holds (via its
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
        let park = Arc::new(ParkControl::new());
        // A published job wakes the idle send worker (nydus-y2e): the publisher
        // half shares this wake through the snapshot channel.
        snapshot_receiver.attach_wake(park.wake());
        Self {
            state: AtomicU8::new(RuntimeState::Armed as u8),
            recv_out_chan_rx: Mutex::new(Some(rx)),
            recv_out_chan_tx: Mutex::new(Some(tx)),
            snapshot_receiver: Mutex::new(Some(snapshot_receiver)),
            shutdown: Arc::new(AtomicBool::new(false)),
            park,
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
        self.park.wake.notify();
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
                        &timing_recv,
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
                        &timing_send,
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
        let expected = u32::try_from(self.workers.lock().len()).expect("worker count fits in u32");
        self.park.park.store(true, Ordering::SeqCst);
        // Wake body-sleeping workers so they loop back to the checkpoint and
        // park immediately. `WorkerWake::notify` holds the wake mutex, so a
        // worker that checked `park=false` under that mutex is either already
        // inside `wait()` (and woken) or sees `park=true` (set above, SeqCst).
        self.park.wake.notify();
        // Event-driven (workers_active) path: wake the active recv worker
        // blocked in its `future::or` select.
        self.park.ping_control();

        let t_barrier = self
            .timing
            .record_barrier
            .map(|_| std::time::Instant::now());
        let mut g = self.park.parked_count.lock();
        while *g < expected {
            // Break early if the not-yet-parked workers have all finished
            // (panicked/exited) — they will never park.
            let finished = self
                .workers
                .lock()
                .iter()
                .filter(|w| {
                    w.join
                        .as_ref()
                        .is_none_or(std::thread::JoinHandle::is_finished)
                })
                .count();
            let finished = u32::try_from(finished).expect("worker count fits in u32");
            if *g + finished >= expected {
                break;
            }
            // Bounded wait so a worker that finishes after the check above (but
            // before we re-loop) is still observed promptly.
            self.park
                .parked_cv
                .wait_for(&mut g, Duration::from_millis(5));
        }
        if let (Some(f), Some(t)) = (self.timing.record_barrier, t_barrier) {
            f(u64::try_from(t.elapsed().as_nanos()).expect("elapsed nanos fit in u64"));
        }
    }

    /// Like [`Self::park_workers`], but first **flushes the send pipeline**: on
    /// return, every snapshot published up to this point (the one-tick-lag
    /// buffer + any pending publish) has been transmitted to io, so a consumer
    /// that now advances its clock + pumps its transport sees them
    /// deterministically. A test/harness barrier for bounded-tick liveness
    /// assertions under active workers.
    ///
    /// Mechanism (race-free, reusing the existing park handshake): quiesce both
    /// workers, request a flush, resume so the send worker drains to io, then
    /// re-park. The final `park_workers()` cannot return until the send worker
    /// reaches its checkpoint — which is strictly AFTER the flush-drain block —
    /// so completion is proof the drain ran. No-op / plain park on
    /// `not(workers_active)` (delivery is already synchronous there).
    pub fn park_workers_flushing(&self) {
        if self.state() != RuntimeState::Running {
            return;
        }
        #[cfg(not(workers_active))]
        {
            self.park_workers();
        }
        #[cfg(workers_active)]
        {
            // 1. Quiesce at checkpoints (deterministic starting point).
            self.park_workers();
            // 2. Arm the flush, then resume: the send worker observes `flush`
            //    (set-before-unpark, so no lost transition) and drains to io.
            self.park.flush.store(true, Ordering::SeqCst);
            self.unpark_workers();
            // 3. Re-park. Its checkpoint-wait completes only once the send
            //    worker has passed the flush-drain block above, so the transmit
            //    is guaranteed on the transport by the time this returns.
            self.park_workers();
            // Defensive: clear in case no send worker consumed it (e.g. it had
            // already exited); a stale `flush` would spend one no-op drain-loop.
            self.park.flush.store(false, Ordering::SeqCst);
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
                .filter(|w| {
                    w.join
                        .as_ref()
                        .is_none_or(std::thread::JoinHandle::is_finished)
                })
                .count();
            let finished_unparked =
                u32::try_from(finished_unparked).expect("worker count fits in u32");
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
        // Wake body-sleeping workers so they observe shutdown.
        self.park.wake.notify();
        // Wake the event-driven (workers_active) recv worker so it leaves its
        // `future::or` and observes shutdown at the loop top.
        self.park.ping_control();

        let mut workers = std::mem::take(&mut *self.workers.lock());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        for w in &mut workers {
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
                    log::warn!(
                        "naia pipeline worker {name} did not exit within 5s; leaking thread"
                    );
                }
            }
        }
        self.state
            .store(RuntimeState::Stopped as u8, Ordering::Release);
    }
}

// ─── Idle wake ───────────────────────────────────────────────────────────────

/// The workers' lost-wakeup-free idle wake (nydus-y2e).
///
/// A worker evaluates its idle condition **under** `mu` and sleeps on `cv`
/// without releasing it until the wait parks. Every producer of a wake
/// (`park_workers`, `Drop`, the test-panic request, `SnapshotSender::send`)
/// writes its state first and then calls [`Self::notify`], which takes `mu`.
/// So a worker that observed "idle" either still holds `mu` (the producer is
/// queued behind it and will notify once the worker is inside the wait) or is
/// already waiting (and is woken). No wake is lost, so no bound is needed.
pub(crate) struct WorkerWake {
    mu: Mutex<()>,
    cv: parking_lot::Condvar,
}

impl WorkerWake {
    pub(crate) fn new() -> Self {
        Self {
            mu: Mutex::new(()),
            cv: parking_lot::Condvar::new(),
        }
    }

    /// Wake every idle worker. Holds `mu` so the wake cannot slip between a
    /// worker's condition check and its wait.
    pub(crate) fn notify(&self) {
        let _g = self.mu.lock();
        self.cv.notify_all();
    }

    /// Sleep, unbounded, until `idle()` is false. Returns the number of wakeups
    /// that found `idle()` still true: the idle-wakeup measure the card bounds
    /// at zero.
    pub(crate) fn wait_while(&self, mut idle: impl FnMut() -> bool) -> u64 {
        let mut g = self.mu.lock();
        if !idle() {
            return 0;
        }
        let mut spurious = 0;
        loop {
            self.cv.wait(&mut g);
            if !idle() {
                return spurious;
            }
            spurious += 1;
        }
    }

    /// Sleep at most `bound` while `idle()` is true (one wait; the caller's
    /// loop re-evaluates). For the recv worker on a poll-only transport, which
    /// has to re-poll the socket, and for the degenerate no-handle wait.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    pub(crate) fn wait_while_for(&self, idle: impl FnOnce() -> bool, bound: Duration) {
        let mut g = self.mu.lock();
        if idle() {
            self.cv.wait_for(&mut g, bound);
        }
    }
}

// ─── Handle lease ────────────────────────────────────────────────────────────

/// The park contract carried by a type (nydus-y2e): **the worker owns its
/// handle and lends it to the consumer through the park protocol.**
///
/// The worker holds the handle in `held` for every active iteration (zero
/// slot locks). At the park checkpoint it [`Self::deposit`]s the handle into
/// the shared slot before announcing itself parked, so when
/// [`PipelineRuntime::park_workers`] returns the consumer is guaranteed to
/// find it there. Leaving the checkpoint it [`Self::reclaim`]s. The consumer
/// only ever touches the slot inside a park window, with the worker at its
/// checkpoint, so the two never contend and the slot lock here can block.
///
/// `reclaim` returns `false` when the consumer kept the handle across the
/// window (`take_handles` without `restore_handles`); the worker then idles
/// bounded and retries, exactly as the former `try_lock` spin did.
///
/// Dropping a lease deposits anything still held (worker exit on shutdown or
/// peer loss, or unwind on a worker panic), so the consumer's park-window
/// borrow keeps working after the worker is gone.
pub(crate) struct HandleLease<'a, H> {
    held: Option<H>,
    slot: &'a Arc<Mutex<Option<H>>>,
    #[cfg(any(test, feature = "test_time"))]
    park: &'a ParkControl,
}

impl<'a, H> HandleLease<'a, H> {
    /// A lease that holds nothing yet; the first checkpoint reclaims.
    #[cfg_attr(not(any(test, feature = "test_time")), allow(unused_variables))]
    pub(crate) fn unclaimed(slot: &'a Arc<Mutex<Option<H>>>, park: &'a ParkControl) -> Self {
        Self {
            held: None,
            slot,
            #[cfg(any(test, feature = "test_time"))]
            park,
        }
    }

    fn count_slot_lock(&self) {
        #[cfg(any(test, feature = "test_time"))]
        self.park.slot_locks.fetch_add(1, Ordering::Relaxed);
    }

    /// Lend the handle to the consumer: move it into the slot. No-op when the
    /// lease holds nothing.
    pub(crate) fn deposit(&mut self) {
        if let Some(h) = self.held.take() {
            self.count_slot_lock();
            *self.slot.lock() = Some(h);
        }
    }

    /// Take the handle back from the slot. `true` when the lease now holds it.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    pub(crate) fn reclaim(&mut self) -> bool {
        if self.held.is_none() {
            self.count_slot_lock();
            self.held = self.slot.lock().take();
        }
        self.held.is_some()
    }

    /// The held handle, if any.
    #[cfg_attr(not(workers_active), allow(dead_code))]
    pub(crate) fn held_mut(&mut self) -> Option<&mut H> {
        self.held.as_mut()
    }
}

impl<H> Drop for HandleLease<'_, H> {
    fn drop(&mut self) {
        self.deposit();
    }
}

// ─── Worker loops ────────────────────────────────────────────────────────────

/// Top-of-iteration: if `park` is set, deposit the handle, increment
/// `parked_count`, notify main, then wait on `resume_cv` until
/// `unpark_workers()` clears `park`, and reclaim the handle on the way out.
///
/// The deposit happens **before** `parked_count` is incremented, so
/// `park_workers()` cannot return until every handle is in its slot.
fn worker_park_checkpoint<H>(park: &ParkControl, lease: &mut HandleLease<'_, H>) {
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
    lease.deposit();
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
    drop(g);
    // Active workers take their handle back now; the deterministic path never
    // holds it (the consumer drives recv/send synchronously in the window).
    #[cfg(workers_active)]
    {
        let _ = lease.reclaim();
    }
}

/// Recv worker loop. The `RecvHandle` is owned by this worker through a
/// [`HandleLease`] and lent to the consumer only inside park windows; see the
/// lease docs for the contract. Between windows the receive path takes no lock
/// on the slot.
///
/// NOTE (clippy `needless_pass_by_value`): `readiness` is deliberately owned.
/// `spawn_workers` moves it into the `'static` worker thread, so no borrow of
/// the caller can satisfy the lifetime; the lint's `Option<&PacketReadiness>`
/// suggestion does not compile here. The body only needs shared access.
#[cfg_attr(not(workers_active), allow(unused_variables))]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn recv_worker_loop<E: Copy + Eq + Hash + Send + Sync + 'static>(
    recv_slot: &Arc<Mutex<Option<RecvHandle<E>>>>,
    out_tx: &Sender<ReceiveOutput<E>>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    readiness: Option<PacketReadiness>,
    timing: &RuntimeTimingHooks,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    let mut lease = HandleLease::unclaimed(recv_slot, park);
    #[cfg(workers_active)]
    let _ = lease.reclaim();
    loop {
        worker_park_checkpoint(park, &mut lease);
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
        // worker is a pure parking service: sleep until the next park.
        #[cfg(not(workers_active))]
        {
            park.wake.wait_while(|| {
                !park.park.load(Ordering::SeqCst)
                    && !shutdown.load(Ordering::SeqCst)
                    && !test_panic.load(Ordering::SeqCst)
            });
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

            // The consumer kept the handle across the window: wait bounded for
            // the next park (which is when it can come back) and retry.
            if !lease.reclaim() {
                park.wake.wait_while_for(
                    || !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst),
                    Duration::from_micros(100),
                );
                continue;
            }
            let recv = lease.held_mut().expect("reclaim returned true");

            let t_recv = timing.record_recv.map(|_| std::time::Instant::now());
            let output = recv.receive();
            if let (Some(f), Some(t)) = (timing.record_recv, t_recv) {
                f(u64::try_from(t.elapsed().as_nanos()).expect("elapsed nanos fit in u64"));
            }

            // Skip empty outputs (PF1-A): an idle `receive()` still builds a
            // `ReceiveOutput`, and queueing one per iteration into the
            // tick-drained unbounded channel is pure heap growth between
            // drains. `is_empty` already blesses skipping as safe.
            // Unbounded channel: NEVER drop a non-empty `ReceiveOutput` — it
            // already holds command packets pulled off the socket. `send`
            // only errors if the receiver hung up (teardown) — exit cleanly
            // then (the lease's Drop deposits the handle).
            if !output.is_empty() && out_tx.send(output).is_err() {
                return;
            }
            // Idle inter-iteration wait.
            if let Some(readiness) = &readiness {
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
                if !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst) {
                    smol::block_on(smol::future::or(readiness.wait(), async {
                        let _ = park.control_rx.recv().await;
                    }));
                }
                // Terminal peer loss (PF1-B): a closed readiness resolves
                // instantly every iteration. Exit instead of spinning — the
                // lease deposits the handle on drop, so the consumer's
                // park-window drain keeps working after us.
                if readiness.is_closed() {
                    return;
                }
                // Clear the token(s) that woke us so the next wait starts
                // fresh (data is drained by recv.receive() at the loop top).
                readiness.drain();
                while park.control_rx.try_recv().is_ok() {}
            // Poll-only transport (raw socket, no awaitable readiness): keep
            // the bounded wait — park_workers()'s wake still cuts the park
            // barrier; the 100µs bound drains the socket.
            } else {
                park.wake.wait_while_for(
                    || !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst),
                    Duration::from_micros(100),
                );
            }
        } // end #[cfg(workers_active)]
    }
}

/// Transmit one frozen job on the held send handle.
#[cfg(workers_active)]
fn transmit_job<E: Copy + Eq + Hash + Send + Sync + 'static>(
    send: &mut SendHandle<E>,
    mut job: SnapshotWorld<E>,
) {
    // Dispatch on the prepared send plan. An active job carries a `SendPlan`
    // built on MAIN at the freeze point (frozen DiffMasks + frozen dirty domain
    // + live masks already cleared), so this lagged transmit reads ZERO live
    // per-user diff state. A job without a plan (not expected on this path)
    // falls back to a synchronous prepare+transmit.
    let _ = job.take_frozen_dirty();
    match job.take_send_plan() {
        Some(plan) => {
            // Drain the ACK channel in the worker preamble (before transmit) so
            // `sent_updates` is consumed on the send side — single-owner. The
            // no-plan fallback below drains inside `send_all_packets`.
            send.drain_all_acks();
            send.transmit_send_job(&job, plan);
        }
        None => send.send_all_packets(&job),
    }
}

/// Send worker loop. Symmetric to [`recv_worker_loop`]: the `SendHandle` is
/// owned through a [`HandleLease`] and lent to the consumer inside park
/// windows. Idle, the worker sleeps unbounded on the [`WorkerWake`]; a
/// snapshot publish, a park request or shutdown wakes it (nydus-y2e: zero idle
/// wakeups).
///
/// In `not(workers_active)` (deterministic) mode this worker is a **pure parking
/// service**: the consumer drives the send synchronously inside its park window,
/// so `snap_rx` / `send_slot` are unused here.
#[cfg_attr(not(workers_active), allow(unused_variables))]
pub(crate) fn send_worker_loop<E: Copy + Eq + Hash + Send + Sync + 'static>(
    send_slot: &Arc<Mutex<Option<SendHandle<E>>>>,
    snap_rx: &SnapshotReceiver<E>,
    shutdown: &Arc<AtomicBool>,
    park: &Arc<ParkControl>,
    timing: &RuntimeTimingHooks,
    #[cfg(any(test, feature = "test_time"))] test_panic: &Arc<AtomicBool>,
) {
    let mut lease = HandleLease::unclaimed(send_slot, park);
    #[cfg(workers_active)]
    let _ = lease.reclaim();
    // MISSION_TICK_FLOOR Lever 3: one-tick send lag. The job published this
    // cycle is buffered here and transmitted on the NEXT cycle, so the worker
    // sends the previous tick's frozen job. The frozen rider prevents a torn
    // read of the live `global_dirty`.
    #[cfg(workers_active)]
    let mut held_job: Option<SnapshotWorld<E>> = None;
    loop {
        worker_park_checkpoint(park, &mut lease);
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(any(test, feature = "test_time"))]
        if test_panic.load(Ordering::SeqCst) {
            panic!("test-requested send worker panic");
        }

        // not(workers_active): pure parking service (see fn docs). The consumer
        // drives the send synchronously inside its park window so handshake +
        // snapshot delivery land at a deterministic point each tick. Sleep
        // until the next park (zero CPU); never touch the snapshot or handle.
        #[cfg(not(workers_active))]
        {
            park.wake.wait_while(|| {
                !park.park.load(Ordering::SeqCst)
                    && !shutdown.load(Ordering::SeqCst)
                    && !test_panic.load(Ordering::SeqCst)
            });
            continue;
        }

        #[cfg(workers_active)]
        {
            // FLUSH-PARK barrier (`park_workers_flushing`): drain the one-tick-lag
            // buffer plus every published-but-untransmitted snapshot straight to
            // io, THEN loop back so the pending park request quiesces us. This
            // makes snapshot delivery deterministic under active workers (a
            // bounded-tick liveness poll can otherwise race the free-running send
            // thread). Only the WHEN of transmit changes — the (snapshot, plan)
            // bytes are byte-identical to the normal lagged path below — and this
            // arm compiles out entirely on not(workers_active), so the
            // deterministic determinism moat is untouched. Bounded: `held_job`
            // holds ≤1 job and `snap_rx` is latest-wins (≤1), so ≤2 transmits.
            if park.flush.swap(false, Ordering::SeqCst) {
                loop {
                    let job_to_send = match snap_rx.take_latest() {
                        Some(new_job) => held_job.replace(new_job),
                        None => held_job.take(),
                    };
                    if let Some(job) = job_to_send {
                        // The flush is driven from inside the consumer's tick,
                        // so the handle is ours unless it was kept across a
                        // window; wait bounded for it rather than drop the job.
                        while !lease.reclaim() {
                            park.wake.wait_while_for(
                                || !shutdown.load(Ordering::SeqCst),
                                Duration::from_micros(100),
                            );
                            if shutdown.load(Ordering::SeqCst) {
                                return;
                            }
                        }
                        transmit_job(lease.held_mut().expect("reclaim returned true"), job);
                    }
                    // Drained once the lag buffer AND the publish slot are empty.
                    if held_job.is_none() && !snap_rx.has_pending() {
                        break;
                    }
                }
                continue;
            }

            // Don't start send work if park was requested — send_all_packets
            // holds time_manager.read() for its entire loop, which can block the
            // recv worker's time_manager.write(); skipping lets this worker reach
            // its checkpoint without contention. (T1)
            if park.park.load(Ordering::SeqCst) {
                continue;
            }

            // The consumer kept the handle across the window: wait bounded for
            // the next park and retry. The lag buffer is untouched, so no job
            // is dropped.
            if !lease.reclaim() {
                park.wake.wait_while_for(
                    || !park.park.load(Ordering::SeqCst) && !shutdown.load(Ordering::SeqCst),
                    Duration::from_micros(100),
                );
                continue;
            }

            // One-tick lag: buffer this cycle's freshly-published job; transmit
            // the job buffered on the PREVIOUS cycle. `replace` returns the prior
            // buffered job (to send now); when no new job was published, flush
            // whatever is still buffered.
            let job_to_send = match snap_rx.take_latest() {
                Some(new_job) => held_job.replace(new_job),
                None => held_job.take(),
            };

            if let Some(job) = job_to_send {
                let t_send = timing.record_send.map(|_| std::time::Instant::now());
                transmit_job(lease.held_mut().expect("reclaim returned true"), job);
                if let (Some(f), Some(t)) = (timing.record_send, t_send) {
                    f(u64::try_from(t.elapsed().as_nanos()).expect("elapsed nanos fit in u64"));
                }
            } else if held_job.is_none() {
                // Nothing published and nothing buffered: sleep unbounded until
                // a publish, a park request or shutdown wakes us. The condition
                // is read under the wake mutex, which every one of those
                // producers takes in `notify`, so no wake is lost.
                let spurious = park.wake.wait_while(|| {
                    #[cfg(any(test, feature = "test_time"))]
                    if test_panic.load(Ordering::SeqCst) {
                        return false;
                    }
                    !park.park.load(Ordering::SeqCst)
                        && !shutdown.load(Ordering::SeqCst)
                        && !snap_rx.has_pending()
                });
                #[cfg(any(test, feature = "test_time"))]
                park.send_idle_wakeups
                    .fetch_add(spurious, Ordering::Relaxed);
                #[cfg(not(any(test, feature = "test_time")))]
                let _ = spurious;
            }
            // else: a job was just buffered (one-tick warmup); loop so the next
            // iteration transmits it, exactly as the former 100µs poll did.
        }
    }
}
