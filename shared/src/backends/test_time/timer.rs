use std::time::Duration;

use naia_socket_shared::Instant;

/// A Timer with a given duration after which it will enter into a "Ringing"
/// state. The Timer can be reset at an given time, or manually set to start
/// "Ringing" again.
pub struct Timer {
    duration: Duration,
    last: Instant,
}

impl Timer {
    /// Creates a new Timer with a given Duration
    pub fn new(duration: Duration) -> Self {
        Self {
            last: Instant::now(),
            duration,
        }
    }

    /// Reset the Timer to stop ringing and wait till 'Duration' has elapsed
    /// again
    pub fn reset(&mut self) {
        self.last = Instant::now();
    }

    /// Gets whether or not the Timer is "Ringing" (i.e. the given Duration has
    /// elapsed since the last "reset")
    pub fn ringing(&self) -> bool {
        let now = Instant::now();
        
        // Special case: zero duration timers should ring immediately after ring_manual()
        if self.duration.as_millis() == 0 {
            // If last is not after now (i.e., last <= now), the timer should ring
            // This handles the case where ring_manual() set last to be in the past
            return !now.is_after(&self.last) || now == self.last;
        }
        
        // Handle case where time might go backwards (shouldn't happen, but be safe)
        if now.is_after(&self.last) {
            self.last.elapsed(&now) > self.duration
        } else {
            false
        }
    }

    /// Manually causes the Timer to enter into a "Ringing" state
    pub fn ring_manual(&mut self) {
        if self.duration.as_millis() > 0 {
            let mut last = self.last.clone();
            last.subtract_millis(self.duration.as_millis() as u32);
            self.last = last;
        } else {
            // For zero duration, set last to be in the past so ringing() returns true
            // Subtract 1ms to ensure last is definitely in the past
            let mut last = Instant::now();
            last.subtract_millis(1);
            self.last = last;
        }
    }
}