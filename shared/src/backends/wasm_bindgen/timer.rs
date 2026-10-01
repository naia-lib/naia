use std::time::Duration;

use naia_socket_shared::monotonic_now_ms;

/// A Timer with a given duration after which it will enter into a "Ringing"
/// state. The Timer can be reset at an given time, or manually set to start
/// "Ringing" again.
///
/// DWO monotonic clock: this reads the one monotonic source behind the wasm
/// time facade ([`monotonic_now_ms`], the `performance.now()` timeline), the
/// same source as `Instant` — never the wall clock — so timers and instants
/// cannot disagree about elapsed time.

pub struct Timer {
    duration: f64,
    last: f64,
}

impl Timer {
    /// Creates a new Timer with a given Duration
    pub fn new(duration: Duration) -> Self {
        Self {
            last: monotonic_now_ms(),
            duration: duration.as_millis() as f64,
        }
    }

    /// Reset the Timer to stop ringing and wait till 'Duration' has elapsed
    /// again
    pub fn reset(&mut self) {
        self.last = monotonic_now_ms();
    }

    /// Gets whether or not the Timer is "Ringing" (i.e. the given Duration has
    /// elapsed since the last "reset")
    pub fn ringing(&self) -> bool {
        (monotonic_now_ms() - self.last) > self.duration
    }

    /// Manually causes the Timer to enter into a "Ringing" state
    pub fn ring_manual(&mut self) {
        self.last -= self.duration;
    }
}
