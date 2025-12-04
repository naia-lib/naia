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
        
        // Special case: zero duration timers should ring immediately
        // After ring_manual(), last is set to be in the past (now - 1ms)
        // After reset(), last is set to now
        // For zero duration, if last <= now, the timer should ring
        if self.duration.as_millis() == 0 {
            // For zero duration: if last is in the past or equal to now, ring immediately
            // now.is_after(&last) returns true if now > last (i.e., last is in the past)
            // If last == now, now.is_after(&last) returns false
            // So: if last is in the past OR last == now, we should ring
            // This is: !now.is_after(&last) || (last == now)
            // But since we want to ring if last <= now, we can use: !now.is_after(&last)
            // However, if last is in the past, now.is_after(&last) is true, so !now.is_after(&last) is false - WRONG!
            // We need: if last <= now, ring. So: !(now < last), which is: now >= last
            // In terms of is_after: if now.is_after(&last) is false, then now <= last, so we should ring
            // But wait, is_after means "is self > other", so if now.is_after(&last) is false, then now <= last
            // So !now.is_after(&last) means now <= last, which is what we want!
            // But the issue is: if last is in the past, now.is_after(&last) is true, so !now.is_after(&last) is false
            // So the logic is wrong. Let me think again...
            // 
            // We want: ring if last <= now
            // is_after: self.is_after(&other) means self > other
            // So now.is_after(&last) means now > last (i.e., last is in the past)
            // We want to ring if now >= last, which is: !(now < last)
            // But we don't have is_before, we have is_after
            // If now.is_after(&last) is false, then now <= last, so we should ring
            // But if now.is_after(&last) is true, then now > last, so we should also ring (last is in the past)
            // So we should ALWAYS ring for zero duration? No, that's not right either.
            // 
            // Actually, the issue is simpler: for zero duration, we want to ring immediately after reset()
            // After reset(), last = now, so we want to ring
            // After ring_manual(), last = now - 1ms, so we want to ring
            // So we want to ring if last <= now
            // now.is_after(&last) returns true if now > last
            // So if now.is_after(&last) is false, then now <= last, which means we should ring
            // But if now.is_after(&last) is true, then now > last (last is in the past), which also means we should ring
            // So we should ALWAYS ring? No wait, that doesn't make sense.
            // 
            // Let me think about this differently:
            // - After reset(): last = now, so we want ringing() to return true
            // - After ring_manual(): last = now - 1ms, so we want ringing() to return true
            // - If time advances and last < now, we still want to ring (zero duration means immediate)
            // So we want: ring if last <= now
            // 
            // The simplest check: compare millis directly
            // But Instant doesn't expose millis... wait, in the test backend it does!
            // Actually, let's use elapsed: if elapsed >= 0, ring
            let elapsed = self.last.elapsed(&now);
            return elapsed >= Duration::ZERO; // Always true for zero duration after reset or ring_manual
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