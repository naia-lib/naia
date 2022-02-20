use std::time::Duration;

use naia_shared::{Tick, Timer};

/// Manages the current tick for the host
pub struct TickManager {
    current_tick: Tick,
    timer: Timer,
}

impl TickManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            current_tick: 0,
            timer: Timer::new(tick_interval),
        }
    }

    /// Whether or not we should emit a tick event
    pub fn receive_tick(&mut self) -> bool {
        if self.timer.ringing() {
            self.timer.reset();
            self.current_tick = self.current_tick.wrapping_add(1);
            return true;
        }
        return false;
    }

    /// Gets the current tick on the host
    pub fn server_tick(&self) -> Tick {
        self.current_tick
    }
}
