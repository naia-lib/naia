use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

// Global simulated clock state
static SIMULATED_CLOCK: AtomicU64 = AtomicU64::new(u64::MAX); // Use MAX as "uninitialized" sentinel

pub struct TestClock;

impl TestClock {
    /// Initialize the simulated clock with a starting time
    ///
    /// This function is idempotent - if called multiple times, it will reset the clock
    /// to the new initial value. This allows tests to reinitialize the clock between
    /// test scenarios.
    pub fn init(initial_ms: u64) {
        SIMULATED_CLOCK.store(initial_ms, Ordering::SeqCst);
    }

    /// Advance the simulated clock by the specified number of milliseconds
    ///
    /// # Panics
    ///
    /// This function will panic if the clock has not been initialized via `init_test_clock`.
    pub fn advance(delta_ms: u64) {
        let current = SIMULATED_CLOCK.load(Ordering::SeqCst);
        if current == u64::MAX {
            panic!("test clock not initialized! Call init_test_clock() first.");
        }
        SIMULATED_CLOCK.store(current + delta_ms, Ordering::SeqCst);
    }

    /// Reset the simulated clock (for cleanup between tests)
    ///
    /// This is useful for test frameworks that need to reset state between tests.
    pub fn reset() {
        SIMULATED_CLOCK.store(u64::MAX, Ordering::SeqCst);
    }

    /// Get the current simulated time in milliseconds
    ///
    /// # Panics
    ///
    /// This function will panic if the clock has not been initialized.
    pub fn current_time_ms() -> u64 {
        let millis = SIMULATED_CLOCK.load(Ordering::SeqCst);
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
    ///
    /// # Panics
    ///
    /// This function will panic if the simulated clock has not been initialized
    /// via `init_test_clock()`.
    pub fn now() -> Self {
        let millis = SIMULATED_CLOCK.load(Ordering::SeqCst);
        if millis == u64::MAX {
            panic!("test clock not initialized! Call init_test_clock() before using Instant::now() in tests.");
        }
        Self {
            millis_since_start: millis,
        }
    }

    /// Returns time elapsed since the Instant
    pub fn elapsed(&self, now: &Self) -> Duration {
        if now.millis_since_start >= self.millis_since_start {
            Duration::from_millis(now.millis_since_start - self.millis_since_start)
        } else {
            // Time went backwards (shouldn't happen with monotonic clock, but handle gracefully)
            Duration::ZERO
        }
    }

    /// Returns time until the Instant occurs
    pub fn until(&self, now: &Self) -> Duration {
        if self.millis_since_start >= now.millis_since_start {
            Duration::from_millis(self.millis_since_start - now.millis_since_start)
        } else {
            // Time already passed
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
    ///
    /// This method exists for API compatibility but always panics in the test backend
    /// since there is no underlying std::time::Instant.
    pub fn inner(&self) -> std::time::Instant {
        panic!("inner() is not available in test backend. Use the Instant API directly.");
    }
}
