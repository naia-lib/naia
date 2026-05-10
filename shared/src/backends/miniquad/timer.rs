extern "C" {
    pub fn naia_now() -> f64;
}

use std::time::Duration;

/// A Timer with a given duration after which it will enter into a "Ringing"
/// state. The Timer can be reset at an given time, or manually set to start
/// "Ringing" again.

pub struct Timer {
    duration: f64,
    last: f64,
}

impl Timer {
    /// Creates a new Timer with a given Duration
    pub fn new(duration: Duration) -> Self {
        // Safety: naia_now() is a pure, no-side-effect extern "C" provided by the miniquad
        // JavaScript runtime. wasm32 is single-threaded; no data races are possible.
        unsafe {
            Self {
                last: naia_now(),
                duration: duration.as_millis() as f64,
            }
        }
    }

    /// Reset the Timer to stop ringing and wait till 'Duration' has elapsed
    /// again
    pub fn reset(&mut self) {
        // Safety: see Timer::new above.
        unsafe {
            self.last = naia_now();
        }
    }

    /// Gets whether or not the Timer is "Ringing" (i.e. the given Duration has
    /// elapsed since the last "reset")
    pub fn ringing(&self) -> bool {
        // Safety: see Timer::new above.
        unsafe { (naia_now() - self.last) > self.duration }
    }

    /// Manually causes the Timer to enter into a "Ringing" state
    pub fn ring_manual(&mut self) {
        self.last -= self.duration;
    }
}
