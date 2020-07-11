use std::time::Duration;

use naia_shared::Instant;

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    current_tick: u16,
    last_instant: Instant,
    last_leftover: Duration,
}

impl ClientTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ClientTickManager {
            tick_interval,
            current_tick: 0,
            last_instant: Instant::now(),
            last_leftover: Duration::new(0, 0),
        }
    }

    /// Gets the current tick for the host
    pub fn get_tick(&self) -> u16 {
        self.current_tick
    }

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn update_frame(&mut self) {
        let mut time_elapsed = self.last_instant.elapsed() - self.last_leftover;
        while time_elapsed > self.tick_interval {
            self.current_tick += 1;
            time_elapsed -= self.tick_interval;
        }

        self.last_leftover = time_elapsed;
        self.last_instant = Instant::now();
    }
}
