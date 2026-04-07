use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// Global simulated clock — visible to all threads so background executor
// threads can call Instant::now() without per-thread initialization.
static SIMULATED_CLOCK: AtomicU64 = AtomicU64::new(u64::MAX); // MAX = uninitialized sentinel

pub struct TestClock;

impl TestClock {
    /// Initialize the simulated clock with a starting time.
    pub fn init(initial_ms: u64) {
        SIMULATED_CLOCK.store(initial_ms, Ordering::Release);
    }

    /// Advance the simulated clock by the specified number of milliseconds.
    pub fn advance(delta_ms: u64) {
        let current = SIMULATED_CLOCK.load(Ordering::Acquire);
        if current == u64::MAX {
            panic!("test clock not initialized! Call TestClock::init() first.");
        }
        SIMULATED_CLOCK.store(current + delta_ms, Ordering::Release);
    }

    /// Reset the simulated clock (for cleanup between tests).
    pub fn reset() {
        SIMULATED_CLOCK.store(u64::MAX, Ordering::Release);
    }

    /// Get the current simulated time in milliseconds.
    pub fn current_time_ms() -> u64 {
        let millis = SIMULATED_CLOCK.load(Ordering::Acquire);
        if millis == u64::MAX {
            panic!("test clock not initialized! Call TestClock::init() first.");
        }
        millis
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
