use std::time::Duration;

use naia_shared::{wrapping_diff, Instant};

/// Manages the current tick for the host
pub struct TickManager {
    tick_interval_millis: f32,
    tick_interval_seconds: f32,
    server_tick: u16,
    client_tick_adjust: u16,
    server_tick_adjust: u16,
    server_tick_running_diff: i16,
    last_tick_instant: Instant,
    pub fraction: f32,
    accumulator: f32,
    minimum_latency: f32,
}

impl TickManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration, minimum_latency_duration: Option<Duration>) -> Self {
        let minimum_latency = {
            if let Some(min_latency) = minimum_latency_duration {
                min_latency.as_millis() as f32
            } else {
                0.0
            }
        };

        let tick_interval_millis = tick_interval.as_millis() as f32;

        TickManager {
            tick_interval_millis,
            tick_interval_seconds: tick_interval.as_nanos() as f32 / 1000000000.0,
            server_tick: 1,
            client_tick_adjust: 0,
            server_tick_adjust: 0,
            server_tick_running_diff: 0,
            last_tick_instant: Instant::now(),
            accumulator: 0.0,
            fraction: 0.0,
            minimum_latency,
        }
    }

    pub fn mark_frame(&mut self) -> bool {
        let mut ticked = false;
        let mut frame_time = self.last_tick_instant.elapsed().as_nanos() as f32 / 1000000000.0;
        if frame_time > 0.25 {
            frame_time = 0.25;
        }
        self.accumulator += frame_time;
        self.last_tick_instant = Instant::now();
        if self.accumulator >= self.tick_interval_seconds {
            while self.accumulator >= self.tick_interval_seconds {
                self.accumulator -= self.tick_interval_seconds;
            }
            // tick has occurred
            ticked = true;
            self.server_tick = self.server_tick.wrapping_add(1);
        }
        self.fraction = self.accumulator / self.tick_interval_seconds;
        ticked
    }

    /// Use tick data from initial server handshake to set the initial tick
    pub fn set_initial_tick(&mut self, server_tick: u16) {
        self.server_tick = server_tick;
        self.server_tick_adjust = ((3000.0 / self.tick_interval_millis) + 1.0) as u16;
        self.client_tick_adjust = ((5000.0 / self.tick_interval_millis) + 1.0) as u16;
    }

    /// Using information from the Server and RTT/Jitter measurements, determine
    /// the appropriate future intended tick
    pub fn record_server_tick(
        &mut self,
        server_tick: u16,
        ping_average: f32,
        jitter_deviation: f32,
    ) {
        self.server_tick_running_diff += wrapping_diff(self.server_tick, server_tick);

        // Decay the diff so that small fluctuations are acceptable
        if self.server_tick_running_diff > 0 {
            self.server_tick_running_diff = self.server_tick_running_diff.wrapping_sub(1);
        }
        if self.server_tick_running_diff < 0 {
            self.server_tick_running_diff = self.server_tick_running_diff.wrapping_add(1);
        }

        // If the server tick is far off enough, reset to the received server tick
        if self.server_tick_running_diff.abs() > 8 {
            self.server_tick = server_tick;
            self.server_tick_running_diff = 0;
        }

        // Calculate incoming & outgoing jitter buffer tick offsets
        let jitter_based_offset = jitter_deviation * 3.0;
        self.server_tick_adjust =
            (jitter_based_offset / self.tick_interval_millis).ceil() as u16;

        // NOTE: I've struggled multiple times with why (ping_average * 2.0) exists in this
        // calculation, figured it out, then returned to struggle later.
        // This is not a bug!
        // Keep in mind that self.server_tick here is the tick we have RECEIVED from the Server
        // which means that the real current server_tick is likely self.server_tick + ping_average / tick_interval
        // By multiplying the ping average here, we are correcting for our late self.server_tick value
        let target_client_adjust_millis =
            self.minimum_latency.max((ping_average * 2.0) + jitter_based_offset);
        self.client_tick_adjust = (target_client_adjust_millis / self.tick_interval_millis)
            .ceil() as u16;
    }

    /// Gets the server tick with the incoming jitter buffer offset applied
    pub fn get_server_tick(&self) -> u16 {
        return self.server_tick.wrapping_sub(self.server_tick_adjust);
    }

    /// Gets the client tick with the outgoing jitter buffer offset applied
    pub fn get_client_tick(&self) -> u16 {
        return self.server_tick.wrapping_add(self.client_tick_adjust);
    }
}
