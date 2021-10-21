use std::time::Duration;

use naia_shared::{HostTickManager, Timer};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct TickManager {
    tick_interval: Duration,
    current_tick: u16,
    timer: Timer,
}

impl TickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            tick_interval,
            current_tick: 0,
            timer: Timer::new(tick_interval),
        }
    }

    /// Whether or not we should emit a tick event
    pub fn should_tick(&mut self) -> bool {
        if self.timer.ringing() {
            self.timer.reset();
            self.current_tick = self.current_tick.wrapping_add(1);
            return true;
        }
        return false;
    }
}

impl HostTickManager for TickManager {
    fn get_tick(&self) -> u16 {
        self.current_tick
    }
}
