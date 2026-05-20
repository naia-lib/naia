//! Cross-thread, per-stage timing aggregator for the pipeline-overlap
//! validation mission (cyberlith `_AGENTS/MISSION_PIPELINE_PERF_VALIDATION.md`).
//!
//! `bench_profile` (cyberlith) uses a `thread_local!` accumulator, so timings
//! recorded on the Recv/Send worker threads are invisible to the main thread's
//! report. This module is a **process-global, thread-safe** accumulator: any
//! thread records into it, and the driver drains one snapshot at bench end.
//!
//! ## Per-tick attribution (the crux of the measurement)
//!
//! The workers run asynchronously, not in lockstep with `cell.update()`. We do
//! not try to attribute each `recv.receive()` to a specific tick. Instead every
//! stage records its **total busy nanoseconds over the whole window**, and the
//! reporter divides each total by the **same** sim-tick count `N` (the number of
//! `TICK` records — one per `cell.update()`). So "avg per tick" means the same
//! thing — total-busy-ns / N — for every stage and for the end-to-end number,
//! which is the only way `avg_tick` vs `max(stages)` is apples-to-apples.
//!
//! Stages:
//! - `RECV`  — `recv.receive()` busy time (worker thread when active; main-side
//!             straggler drain otherwise).
//! - `SIM`   — the Sim update (cyberlith `do_park_window_tick` Step 4), main thread.
//! - `SEND`  — send preamble + scope + `send_all_packets` (worker thread when active).
//! - `TICK`  — end-to-end `cell.update()` wall-clock, recorded by the driver.
//!             Its **count is the sim-tick count `N`** used as the denominator.
//! - `BARRIER` — time the main thread spends *blocked* inside `park_workers()`
//!             waiting for workers to reach their checkpoint. Large ⇒ the main
//!             thread is waiting on worker work ⇒ stages serialized through the
//!             park barrier. ~0 ⇒ workers finished in the gaps.
//!
//! Gated behind `pipeline_timing`; the `record_*` fns compile to no-ops when off.

/// Record `ns` of busy time against the Recv stage.
#[inline(always)]
pub fn record_recv(ns: u64) {
    #[cfg(feature = "pipeline_timing")]
    imp::RECV.record(ns);
    #[cfg(not(feature = "pipeline_timing"))]
    let _ = ns;
}

/// Record `ns` of busy time against the Sim stage.
#[inline(always)]
pub fn record_sim(ns: u64) {
    #[cfg(feature = "pipeline_timing")]
    imp::SIM.record(ns);
    #[cfg(not(feature = "pipeline_timing"))]
    let _ = ns;
}

/// Record `ns` of busy time against the Send stage.
#[inline(always)]
pub fn record_send(ns: u64) {
    #[cfg(feature = "pipeline_timing")]
    imp::SEND.record(ns);
    #[cfg(not(feature = "pipeline_timing"))]
    let _ = ns;
}

/// Record one end-to-end `cell.update()` of `ns`. Its count is the sim-tick `N`.
#[inline(always)]
pub fn record_tick(ns: u64) {
    #[cfg(feature = "pipeline_timing")]
    imp::TICK.record(ns);
    #[cfg(not(feature = "pipeline_timing"))]
    let _ = ns;
}

/// Record `ns` the main thread spent blocked inside `park_workers()`.
#[inline(always)]
pub fn record_barrier(ns: u64) {
    #[cfg(feature = "pipeline_timing")]
    imp::BARRIER.record(ns);
    #[cfg(not(feature = "pipeline_timing"))]
    let _ = ns;
}

#[cfg(feature = "pipeline_timing")]
mod imp {
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct Stage {
        pub count: AtomicU64,
        pub total_ns: AtomicU64,
    }

    impl Stage {
        const fn new() -> Self {
            Self {
                count: AtomicU64::new(0),
                total_ns: AtomicU64::new(0),
            }
        }
        #[inline]
        pub fn record(&self, ns: u64) {
            self.count.fetch_add(1, Ordering::Relaxed);
            self.total_ns.fetch_add(ns, Ordering::Relaxed);
        }
        fn snapshot(&self) -> (u64, u64) {
            (
                self.count.load(Ordering::Relaxed),
                self.total_ns.load(Ordering::Relaxed),
            )
        }
        fn reset(&self) {
            self.count.store(0, Ordering::Relaxed);
            self.total_ns.store(0, Ordering::Relaxed);
        }
    }

    pub static RECV: Stage = Stage::new();
    pub static SIM: Stage = Stage::new();
    pub static SEND: Stage = Stage::new();
    pub static TICK: Stage = Stage::new();
    pub static BARRIER: Stage = Stage::new();

    pub fn snapshot_all() -> super::PipelineTimingReport {
        let (recv_count, recv_ns) = RECV.snapshot();
        let (sim_count, sim_ns) = SIM.snapshot();
        let (send_count, send_ns) = SEND.snapshot();
        let (tick_count, tick_ns) = TICK.snapshot();
        let (barrier_count, barrier_ns) = BARRIER.snapshot();
        super::PipelineTimingReport {
            recv_count,
            recv_ns,
            sim_count,
            sim_ns,
            send_count,
            send_ns,
            tick_count,
            tick_ns,
            barrier_count,
            barrier_ns,
        }
    }

    pub fn reset_all() {
        RECV.reset();
        SIM.reset();
        SEND.reset();
        TICK.reset();
        BARRIER.reset();
    }
}

/// A drained snapshot of all stage accumulators. All `*_ns` are total busy
/// nanoseconds over the window; all `*_count` are the number of records.
/// `tick_count` is the sim-tick denominator `N`.
#[cfg(feature = "pipeline_timing")]
#[derive(Debug, Clone, Copy)]
pub struct PipelineTimingReport {
    pub recv_count: u64,
    pub recv_ns: u64,
    pub sim_count: u64,
    pub sim_ns: u64,
    pub send_count: u64,
    pub send_ns: u64,
    pub tick_count: u64,
    pub tick_ns: u64,
    pub barrier_count: u64,
    pub barrier_ns: u64,
}

/// Drain a snapshot of every stage accumulator (does not reset).
#[cfg(feature = "pipeline_timing")]
pub fn snapshot() -> PipelineTimingReport {
    imp::snapshot_all()
}

/// Zero every stage accumulator. Call before the measurement window.
#[cfg(feature = "pipeline_timing")]
pub fn reset() {
    imp::reset_all();
}
