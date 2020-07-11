use std::time::Duration;

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    current_tick: u16,
}

impl ClientTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ClientTickManager {
            tick_interval,
            current_tick: 0,
        }
    }

    /// Gets the current tick for the host
    pub fn get_tick(&self) -> u16 {
        self.current_tick
    }

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn update_frame(&mut self) {}
}
