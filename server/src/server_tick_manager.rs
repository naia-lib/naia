use std::time::Duration;

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ServerTickManager {
    tick_interval: Duration,
    current_tick: u16,
}

impl ServerTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ServerTickManager {
            tick_interval,
            current_tick: 0,
        }
    }

    /// Gets the current tick for the host
    pub fn get_tick(&self) -> u16 {
        self.current_tick
    }

    /// Increments the current tick
    pub fn increment_tick(&mut self) {
        self.current_tick = self.current_tick.wrapping_add(1);
    }
}
