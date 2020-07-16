use std::{convert::TryInto, time::Duration};

use naia_shared::{HostTickManager, Instant};

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

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn update_frame(&mut self) {
        let mut time_elapsed = self.last_instant.elapsed() + self.last_leftover;
        if time_elapsed > self.tick_interval {
            while time_elapsed > self.tick_interval {
                self.current_tick = self.current_tick.wrapping_add(1);
                time_elapsed -= self.tick_interval;
            }

            self.last_leftover = time_elapsed;
            self.last_instant = Instant::now();
        }
    }
}

impl HostTickManager for ClientTickManager {
    fn get_tick(&self) -> u16 {
        self.current_tick
    }

    fn process_incoming(&mut self, tick_latency: i8) {
        // The server has told us the latency, adjust current_tick accordingly
        //unimplemented!()
        if tick_latency == -1 {
            return;
        } else if tick_latency < 0 {
            let diff: u16 = (tick_latency * -1).try_into().unwrap();
            println!("host tick should subtract {}", diff);
            self.current_tick = self.current_tick.wrapping_sub(diff);
        } else if tick_latency > 0 {
            let diff: u16 = tick_latency.try_into().unwrap();
            println!("host tick should add {}", diff);
            self.current_tick = self.current_tick.wrapping_add(diff);
        }
    }
}
