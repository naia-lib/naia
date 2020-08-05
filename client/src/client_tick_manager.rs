use std::time::Duration;

use naia_shared::{wrapping_diff, HostTickManager, Instant};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    current_tick: u16,
    last_instant: Instant,
    last_leftover: Duration,
    paused: bool,
    last_received_tick: u16,
    tick_latency_average: f32,
    tick_latency_variance: f32,
    last_tick_latency_average: f32,
    tick_adjust: f32,
    intended_tick: u16,
    processed_first: bool,
    average_pool_size: f32,
    min_target_latency: f32,
    average_adjust: f32,
    sync_config: SyncConfig,
}

/// Holds configuration values that determine factors in how to converge towards
/// the appropriate tick offset
#[derive(Debug)]
pub struct SyncConfig {
    measurement_pool_size: f32,
    target_latency_deviation_multiple: f32,
    adjust_trigger_sensitivity: f32,
    maximum_tick_adjustment: f32,
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
            paused: false,
            last_received_tick: 0,
            tick_adjust: 0.0,
            tick_latency_average: 0.0,
            tick_latency_variance: 0.0,
            last_tick_latency_average: 0.0,
            average_pool_size: 1.0,
            processed_first: false,
            min_target_latency: -1000.0 / (tick_interval.as_millis() as f32),
            average_adjust: 0.0,
            sync_config: SyncConfig {
                measurement_pool_size: 20.0,
                target_latency_deviation_multiple: 3.0,
                adjust_trigger_sensitivity: 0.1,
                maximum_tick_adjustment: 5000.0 / (tick_interval.as_millis() as f32),
            },
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
