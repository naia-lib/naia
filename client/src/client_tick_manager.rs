use std::time::Duration;

use naia_shared::{wrapping_diff, HostTickManager, Instant, StandardHeader};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    current_tick: u16,
    last_instant: Instant,
    last_leftover: Duration,
    paused: bool,
    last_received_tick: u16,
    average_tick_latency: f32,
    last_average_tick_latency: f32,
    tick_adjust: f32,
    intended_tick: u16,
}

impl ClientTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ClientTickManager {
            tick_interval,
            current_tick: 0,
            intended_tick: 1,
            last_instant: Instant::now(),
            last_leftover: Duration::new(0, 0),
            paused: false,
            last_received_tick: 0,
            tick_adjust: 20.0,
            average_tick_latency: -4.0,
            last_average_tick_latency: -4.0,
        }
    }

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn update_frame(&mut self) {
        let mut time_elapsed = self.last_instant.elapsed() + self.last_leftover;
        if time_elapsed > self.tick_interval {
            while time_elapsed > self.tick_interval {
                //TODO: this will have issues when wrapping around..
                if self.current_tick < self.intended_tick - 1 {
                    self.current_tick = self.current_tick.wrapping_add(2);
                } else if self.current_tick > self.intended_tick + 1 {
                    // do nothing
                } else {
                    self.current_tick = self.current_tick.wrapping_add(1);
                }
                time_elapsed -= self.tick_interval;
            }

            self.last_leftover = time_elapsed;
            self.last_instant = Instant::now();
        }
    }

    /// Set current tick
    pub fn set_tick(&mut self, tick: u16) {
        self.current_tick = tick;
    }
}

impl HostTickManager for ClientTickManager {
    fn get_tick(&self) -> u16 {
        self.current_tick
    }

    fn process_incoming(&mut self, header: &StandardHeader) {
        let tick_latency = header.tick_latency();
        if tick_latency == std::i8::MAX {
            return;
        }
        let remote_tick = header.tick();
        let remote_tick_diff = wrapping_diff(self.last_received_tick, remote_tick);

        if remote_tick_diff <= 0 {
            return;
        }
        self.last_received_tick = remote_tick;

        const RATIO: f32 = 40.0;
        self.average_tick_latency =
            ((self.average_tick_latency * RATIO) + f32::from(tick_latency)) / (RATIO + 1.0);
        if self.average_tick_latency > -3.9 {
            if self.last_average_tick_latency - self.average_tick_latency < 0.0 {
                // average tick latency is increasing
                self.tick_adjust += 0.05;
            }
        }
        if self.average_tick_latency < -4.1 && self.tick_adjust > 0.0 {
            if self.last_average_tick_latency - self.average_tick_latency > 0.0 {
                // average tick latency is decreasing
                self.tick_adjust -= 0.05;
            }
        }
        self.last_average_tick_latency = self.average_tick_latency;
        self.intended_tick = remote_tick;
        if self.tick_adjust > 0.0 {
            let diff: u16 = self.tick_adjust as u16; //risky..
            self.intended_tick = self.intended_tick.wrapping_add(diff);
        }
        println!("***");
        println!(
            "AvgTickLat: {}, TickAdj: {}, IntendedTick: {}, CurrentTick: {}",
            self.average_tick_latency, self.tick_adjust, self.intended_tick, self.current_tick
        );
        println!("***");
    }
}
