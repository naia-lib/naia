use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// Thread-local simulated clock — each test thread gets its own independent
// clock so parallel tests never interfere with each other.  The test executor
// is single-threaded per scenario, so cross-thread clock reads are not needed
// by default.
//
// D6 hybrid: callers that need a clock shared across worker threads (e.g.
// cyberlith's `pipelined_recv` worker reading naia's clock via
// `Instant::now()`) can opt in by calling `TestClock::shareable_handle()` on
// the owning thread and `install_shared(arc)` on each worker thread. While a
// shared override is installed on a thread, all TestClock ops route through
// the shared `Arc<AtomicU64>` instead of the thread-local cell. This
// preserves naia's existing parallel-test isolation (default path is
// unchanged) and avoids any process-wide mutex / serialization primitive.
thread_local! {
    static LOCAL_CLOCK: Cell<u64> = const { Cell::new(u64::MAX) }; // MAX = uninitialized sentinel
    static SHARED_OVERRIDE: RefCell<Option<Arc<AtomicU64>>> = const { RefCell::new(None) };
}

pub struct TestClock;

impl TestClock {
    /// Initialize the simulated clock with a starting time.
    pub fn init(initial_ms: u64) {
        SHARED_OVERRIDE.with(|s| {
            if let Some(arc) = s.borrow().as_ref() {
                arc.store(initial_ms, Ordering::SeqCst);
            } else {
                LOCAL_CLOCK.with(|c| c.set(initial_ms));
            }
        });
    }

    /// Advance the simulated clock by the specified number of milliseconds.
    /// Lazy-inits to 0 if no explicit `init()` has been called on this thread
    /// (thread-local path only; shared path is always explicitly initialized).
    pub fn advance(delta_ms: u64) {
        SHARED_OVERRIDE.with(|s| {
            if let Some(arc) = s.borrow().as_ref() {
                arc.fetch_add(delta_ms, Ordering::SeqCst);
            } else {
                LOCAL_CLOCK.with(|c| {
                    let current = c.get();
                    let base = if current == u64::MAX { 0 } else { current };
                    c.set(base + delta_ms);
                });
            }
        });
    }

    /// Reset the simulated clock (for cleanup between tests).
    pub fn reset() {
        SHARED_OVERRIDE.with(|s| {
            if let Some(arc) = s.borrow().as_ref() {
                arc.store(0, Ordering::SeqCst);
            } else {
                LOCAL_CLOCK.with(|c| c.set(u64::MAX));
            }
        });
    }

    /// Get the current simulated time in milliseconds.
    /// Lazy-inits to 0 on first read if no explicit `init()` has been called
    /// on this thread (thread-local path only) — every test thread gets a
    /// sane starting clock without boilerplate. Tests that want a specific
    /// start time still call `TestClock::init(N)` explicitly.
    pub fn current_time_ms() -> u64 {
        SHARED_OVERRIDE.with(|s| {
            if let Some(arc) = s.borrow().as_ref() {
                arc.load(Ordering::SeqCst)
            } else {
                LOCAL_CLOCK.with(|c| {
                    let millis = c.get();
                    if millis == u64::MAX {
                        c.set(0);
                        0
                    } else {
                        millis
                    }
                })
            }
        })
    }

    /// Promote this thread's clock to a process-shareable handle.
    /// Captures the current clock value into a fresh `Arc<AtomicU64>`,
    /// installs the shared override on this thread, and returns the Arc for
    /// passing to spawned worker threads. Workers call `install_shared(arc)`
    /// to read/write the same clock.
    pub fn shareable_handle() -> Arc<AtomicU64> {
        let current = Self::current_time_ms();
        let arc = Arc::new(AtomicU64::new(current));
        Self::install_shared(arc.clone());
        arc
    }

    /// Install a shared clock handle on the calling thread. All subsequent
    /// TestClock operations on this thread route through the Arc.
    pub fn install_shared(arc: Arc<AtomicU64>) {
        SHARED_OVERRIDE.with(|s| *s.borrow_mut() = Some(arc));
    }

    /// Detach the calling thread from any installed shared handle.
    /// Reverts to thread-local clock. Tests/scenarios should call this in
    /// cleanup to avoid leaking the override to the next test that runs on
    /// the same OS thread.
    pub fn detach_shared() {
        SHARED_OVERRIDE.with(|s| *s.borrow_mut() = None);
    }

    /// Returns `true` if a shared clock handle is installed on the calling
    /// thread (via [`install_shared`](Self::install_shared)). The clock is
    /// otherwise thread-local, so a harness that drives a deterministic
    /// scenario across threads (rayon pool, `std::thread::spawn`, …) MUST
    /// share the clock or it silently diverges. Lets such a harness assert the
    /// clock is actually shared before running on a non-owning thread.
    pub fn is_shared() -> bool {
        SHARED_OVERRIDE.with(|s| s.borrow().is_some())
    }
}

/// Represents a specific moment in simulated test time
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Instant {
    millis_since_start: u64,
}

impl Instant {
    /// Creates an Instant from the current simulated time
    pub fn now() -> Self {
        Self {
            millis_since_start: TestClock::current_time_ms(),
        }
    }

    /// Returns time elapsed since the Instant
    pub fn elapsed(&self, now: &Self) -> Duration {
        if now.millis_since_start >= self.millis_since_start {
            Duration::from_millis(now.millis_since_start - self.millis_since_start)
        } else {
            Duration::ZERO
        }
    }

    /// Returns time until the Instant occurs
    pub fn until(&self, now: &Self) -> Duration {
        if self.millis_since_start >= now.millis_since_start {
            Duration::from_millis(self.millis_since_start - now.millis_since_start)
        } else {
            Duration::ZERO
        }
    }

    pub fn is_after(&self, other: &Self) -> bool {
        self.millis_since_start > other.millis_since_start
    }

    /// Adds a given number of milliseconds to the Instant
    pub fn add_millis(&mut self, millis: u32) {
        self.millis_since_start = self.millis_since_start.saturating_add(millis as u64);
    }

    /// Subtracts a given number of milliseconds from the Instant
    pub fn subtract_millis(&mut self, millis: u32) {
        self.millis_since_start = self.millis_since_start.saturating_sub(millis as u64);
    }

    /// Returns inner Instant implementation (not available in test backend)
    pub fn inner(&self) -> std::time::Instant {
        panic!("inner() is not available in test backend. Use the Instant API directly.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::thread;

    // Each test runs on its own spawned thread so the thread-local clock
    // starts from a clean slate regardless of cargo's test scheduler.

    #[test]
    fn default_path_is_thread_local() {
        thread::spawn(|| {
            TestClock::init(100);
            assert_eq!(TestClock::current_time_ms(), 100);

            let worker_value = thread::spawn(TestClock::current_time_ms).join().unwrap();
            // Worker thread has its own LOCAL_CLOCK — uninitialized sentinel
            // lazy-inits to 0 on first read, independent of main.
            assert_eq!(worker_value, 0);

            // Main thread is undisturbed by the worker.
            assert_eq!(TestClock::current_time_ms(), 100);
            TestClock::detach_shared();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn shareable_handle_makes_main_writes_visible_to_worker() {
        thread::spawn(|| {
            TestClock::init(500);
            let arc = TestClock::shareable_handle();

            let arc_for_worker = arc.clone();
            let worker_initial = thread::spawn(move || {
                TestClock::install_shared(arc_for_worker);
                let v = TestClock::current_time_ms();
                TestClock::detach_shared();
                v
            })
            .join()
            .unwrap();
            assert_eq!(worker_initial, 500);

            TestClock::advance(40);

            let arc_for_worker = arc.clone();
            let worker_after_advance = thread::spawn(move || {
                TestClock::install_shared(arc_for_worker);
                let v = TestClock::current_time_ms();
                TestClock::detach_shared();
                v
            })
            .join()
            .unwrap();
            assert_eq!(worker_after_advance, 540);

            // Round-trip: worker advances, main observes.
            let arc_for_worker = arc.clone();
            thread::spawn(move || {
                TestClock::install_shared(arc_for_worker);
                TestClock::advance(10);
                TestClock::detach_shared();
            })
            .join()
            .unwrap();
            assert_eq!(TestClock::current_time_ms(), 550);

            TestClock::detach_shared();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn detach_shared_reverts_to_local() {
        thread::spawn(|| {
            let arc = Arc::new(AtomicU64::new(0));
            TestClock::install_shared(arc.clone());
            TestClock::init(1000);
            assert_eq!(TestClock::current_time_ms(), 1000);
            assert_eq!(arc.load(Ordering::SeqCst), 1000);

            TestClock::detach_shared();
            // Local cell on this fresh thread was never touched by the shared
            // path — still u64::MAX sentinel, which lazy-inits to 0 on read.
            assert_eq!(TestClock::current_time_ms(), 0);
            // And the shared arc still holds its own value, unaffected by
            // reads on the now-detached thread.
            assert_eq!(arc.load(Ordering::SeqCst), 1000);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn reset_routes_through_shared_when_installed() {
        thread::spawn(|| {
            let arc = Arc::new(AtomicU64::new(500));
            TestClock::install_shared(arc.clone());
            assert_eq!(TestClock::current_time_ms(), 500);

            TestClock::reset();
            assert_eq!(arc.load(Ordering::SeqCst), 0);
            assert_eq!(TestClock::current_time_ms(), 0);

            TestClock::detach_shared();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn naia_existing_pattern_unaffected() {
        // Exercises the legacy thread_local-only flow exactly as naia's
        // pre-D6 tests did: init → advance → read, no shared handle.
        thread::spawn(|| {
            TestClock::init(0);
            assert_eq!(TestClock::current_time_ms(), 0);
            TestClock::advance(25);
            assert_eq!(TestClock::current_time_ms(), 25);
            TestClock::advance(75);
            assert_eq!(TestClock::current_time_ms(), 100);
            TestClock::reset();
            // After reset, sentinel lazy-inits back to 0 on next read.
            assert_eq!(TestClock::current_time_ms(), 0);
        })
        .join()
        .unwrap();
    }
}
