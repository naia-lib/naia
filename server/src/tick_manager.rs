use std::time::Duration;

use naia_shared::HostTickManager;

/// Manages the current tick for the host
#[derive(Debug)]
pub struct TickManager {
    tick_interval: Duration,
    current_tick: u16,
}

impl TickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            tick_interval,
            current_tick: 0,
        }
    }

    /// Increments the current tick
    pub fn increment_tick(&mut self) {
        self.current_tick = self.current_tick.wrapping_add(1);
    }
}

impl HostTickManager for TickManager {
    fn get_tick(&self) -> u16 {
        self.current_tick
    }
}
