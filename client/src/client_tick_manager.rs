use std::time::Duration;

use naia_shared::{wrapping_diff, HostTickManager, Instant};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct ClientTickManager {
    tick_interval: Duration,
    client_tick: u16,
    server_tick: u16,
    client_tick_adjust: u16,
    server_tick_adjust: u16,
    last_client_tick_instant: Instant,
    last_client_tick_leftover: Duration,
    last_server_tick_instant: Instant,
    last_server_tick_leftover: Duration,
    server_tick_running_diff: i16,
}

const NANOS_PER_SEC: u32 = 1_000_000_000;

impl ClientTickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        ClientTickManager {
            tick_interval,
            client_tick: 0,
            server_tick: 1,
            last_client_tick_instant: Instant::now(),
            last_client_tick_leftover: Duration::new(0, 0),
            last_server_tick_instant: Instant::now(),
            last_server_tick_leftover: Duration::new(0, 0),
            client_tick_adjust: 0,
            server_tick_adjust: 0,
            server_tick_running_diff: 0,
        }
    }

    /// If the tick interval duration has elapsed, increment the current tick
    pub fn has_ticked(&mut self) -> bool {
        // Server Tick
        {
            let mut time_elapsed =
                self.last_server_tick_instant.elapsed() + self.last_server_tick_leftover;

            if time_elapsed > self.tick_interval {
                while time_elapsed > self.tick_interval {
                    self.server_tick = self.server_tick.wrapping_add(1);
                    time_elapsed -= self.tick_interval;
                }

                self.last_server_tick_leftover = time_elapsed;
                self.last_server_tick_instant = Instant::now();
            }
        }

        // Client Tick
        {
            let mut time_elapsed =
                self.last_client_tick_instant.elapsed() + self.last_client_tick_leftover;

            let intended_diff = wrapping_diff(
                self.client_tick,
                self.server_tick.wrapping_add(self.client_tick_adjust),
            )
            .min(20)
            .max(-20);
            let tick_factor = 2.0_f64.powf(-0.2_f64 * f64::from(intended_diff));
            let adjusted_interval = self.get_adjusted_duration(tick_factor);

            if time_elapsed > adjusted_interval {
                self.client_tick = self.client_tick.wrapping_add(1);
                time_elapsed -= adjusted_interval;

                self.last_client_tick_leftover = time_elapsed;
                self.last_client_tick_instant = Instant::now();

                return true;
            } else {
                return false;
            }
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

    /// Use tick data from initial server handshake to set the initial tick
    pub fn set_initial_tick(&mut self, server_tick: u16) {
        self.server_tick = server_tick;
        self.server_tick_adjust = ((1000 / (self.tick_interval.as_millis())) + 1) as u16;

        self.client_tick_adjust = ((3000 / (self.tick_interval.as_millis())) + 1) as u16;
        self.client_tick = server_tick.wrapping_add(self.client_tick_adjust);

        self.last_server_tick_instant = Instant::now();
        self.last_client_tick_instant = Instant::now();
    }

    /// Using information from the Server and RTT/Jitter measurements, determine
    /// the appropriate future intended tick
    pub fn record_server_tick(
        &mut self,
        server_tick: u16,
        rtt_average: f32,
        jitter_deviation: f32,
    ) {
        self.server_tick_running_diff += wrapping_diff(self.server_tick, server_tick);
        if self.server_tick_running_diff > 0 {
            if self.server_tick_running_diff > 8 {
                println!(
                    "Adding! Client: {}, Server: {}",
                    self.server_tick, server_tick,
                );
                self.server_tick = self.server_tick.wrapping_add(1);
                self.server_tick_running_diff = 0;
            }

            self.server_tick_running_diff = self.server_tick_running_diff.wrapping_sub(1);
        }
        if self.server_tick_running_diff < 0 {
            if self.server_tick_running_diff < -8 {
                println!(
                    "Subing! Client: {}, Server: {}",
                    self.server_tick, server_tick,
                );
                self.server_tick = self.server_tick.wrapping_sub(1);
                self.server_tick_running_diff = 0;
            }

            self.server_tick_running_diff = self.server_tick_running_diff.wrapping_add(1);
        }

        self.server_tick_adjust =
            ((((jitter_deviation * 3.0) / 2.0) / self.tick_interval.as_millis() as f32) + 1.0)
                .ceil() as u16;
        self.client_tick_adjust = (((rtt_average + (jitter_deviation * 3.0) / 2.0)
            / (self.tick_interval.as_millis() as f32))
            + 1.0)
            .ceil() as u16;
    }

    /// Gets a reference to the tick interval used
    pub fn get_tick_interval(&self) -> &Duration {
        return &self.tick_interval;
    }

    /// Gets the server tick with the jitter buffer offset applied
    pub fn get_buffered_server_tick(&self) -> u16 {
        return self.server_tick.wrapping_sub(self.server_tick_adjust);
    }
}

impl HostTickManager for ClientTickManager {
    fn get_tick(&self) -> u16 {
        self.client_tick
    }
}
