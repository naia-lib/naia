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

// DWO clock browser proof, naia-shared side: this Timer paces the
// production handshake (`send()` retransmits), so it is pinned directly
// against the live performance.now timeline — no sleeps, no frozen clock.
// A bounded spin covers the sub-millisecond gap between construction and
// the strictly-elapsed ring.
#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_timer_tests {
    use std::time::Duration;

    use wasm_bindgen_test::wasm_bindgen_test;

    use super::Timer;
    use crate::Instant;

    /// Upper bound on spins waiting for the live clock to tick past the
    /// construction instant; each spin re-reads performance.now, so this
    /// only exhausts on a clock that never advances at all.
    const MAX_RING_SPINS: u32 = 100_000;

    fn spin_until_ringing(timer: &Timer) {
        for _ in 0..MAX_RING_SPINS {
            if timer.ringing() {
                return;
            }
        }
        panic!("wasm Timer never rang on the live clock");
    }

    #[wasm_bindgen_test]
    fn ring_manual_rings_on_live_clock() {
        let mut timer = Timer::new(Duration::from_secs(3600));
        timer.ring_manual();
        spin_until_ringing(&timer);
    }

    #[wasm_bindgen_test]
    fn fresh_reset_is_silent() {
        let mut timer = Timer::new(Duration::from_secs(3600));
        timer.reset();
        // A just-reset hour timer cannot have strictly elapsed.
        assert!(!timer.ringing());
    }

    #[wasm_bindgen_test]
    fn timer_agrees_with_instant_facade() {
        let before = Instant::now();
        let mut timer = Timer::new(Duration::from_secs(3600));
        timer.ring_manual();
        spin_until_ringing(&timer);
        // Same underlying source: the Instant facade never steps backwards
        // across the Timer's ring.
        assert!(Instant::now() >= before);
    }
}
