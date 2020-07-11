use std::time::Duration;

#[derive(Debug)]
pub struct TickManager {
    tick_interval: Duration,
    current_tick: u16,
    tick_latency: u8,
}

impl TickManager {
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            tick_interval,
            current_tick: 0,
            tick_latency: 0,
        }
    }

    pub fn get_current_tick(&self) -> u16 {
        self.current_tick
    }

    pub fn get_tick_latency(&self) -> u8 {
        self.tick_latency
    }
}
