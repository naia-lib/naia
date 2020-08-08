use std::time::Duration;

use naia_shared::{wrapping_diff, HostTickManager, Instant};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    current_tick: u16,
    intended_tick: u16,
    last_instant: Instant,
    last_leftover: Duration,
}

const NANOS_PER_SEC: u32 = 1_000_000_000;

impl ClientTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ClientTickManager {
            tick_interval,
            current_tick: 0,
            intended_tick: 1,
            last_instant: Instant::now(),
            last_leftover: Duration::new(0, 0),
        }
    }

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn update_frame(&mut self) {
        let mut time_elapsed = self.last_instant.elapsed() + self.last_leftover;

        let intended_diff = wrapping_diff(self.current_tick, self.intended_tick)
            .min(16)
            .max(-8);
        let tick_factor = 2.0_f64.powf(-0.2_f64 * f64::from(intended_diff));
        let adjusted_interval = self.get_adjusted_duration(tick_factor);

        if time_elapsed > adjusted_interval {
            while time_elapsed > adjusted_interval {
                self.current_tick = self.current_tick.wrapping_add(1);
                time_elapsed -= adjusted_interval;
            }

            self.last_leftover = time_elapsed;
            self.last_instant = Instant::now();
        }
    }

    fn get_adjusted_duration(&self, tick_factor: f64) -> Duration {
        const MAX_NANOS_F64: f64 = ((std::u64::MAX as u128 + 1) * (NANOS_PER_SEC as u128)) as f64;
        let nanos = tick_factor * self.tick_interval.as_secs_f64() * (NANOS_PER_SEC as f64);
        if !nanos.is_finite() {
            panic!("got non-finite value when converting float to duration");
        }
        if nanos >= MAX_NANOS_F64 {
            panic!("overflow when converting float to duration");
        }
        if nanos < 0.0 {
            panic!("underflow when converting float to duration");
        }
        let nanos_u128 = nanos as u128;
        Duration::new(
            (nanos_u128 / (NANOS_PER_SEC as u128)) as u64,
            (nanos_u128 % (NANOS_PER_SEC as u128)) as u32,
        )
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
}
