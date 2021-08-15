use std::time::Duration;

use naia_shared::{wrapping_diff, Instant};

/// Manages the current tick for the host
#[derive(Debug)]
pub struct TickManager {
    tick_interval: Duration,
    tick_interval_f32: f32,
    server_tick: u16,
    client_tick_adjust: u16,
    server_tick_adjust: u16,
    server_tick_running_diff: i16,
    last_tick_instant: Instant,
    pub fraction: f32,
    accumulator: f32,
}

impl TickManager {
    /// Create a new HostTickManager with a given tick interval duration
    pub fn new(tick_interval: Duration) -> Self {
        TickManager {
            tick_interval,
            tick_interval_f32: tick_interval.as_nanos() as f32 / 1000000000.0,
            server_tick: 1,
            client_tick_adjust: 0,
            server_tick_adjust: 0,
            server_tick_running_diff: 0,
            last_tick_instant: Instant::now(),
            accumulator: 0.0,
            fraction: 0.0,
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
        if self.accumulator >= self.tick_interval_f32 {
            while self.accumulator >= self.tick_interval_f32 {
                self.accumulator -= self.tick_interval_f32;
            }
            // tick has occurred
            ticked = true;
            self.server_tick = self.server_tick.wrapping_add(1);
        }
        self.fraction = self.accumulator / self.tick_interval_f32;
        ticked
    }

    /// Use tick data from initial server handshake to set the initial tick
    pub fn set_initial_tick(&mut self, server_tick: u16) {
        self.server_tick = server_tick;
        self.server_tick_adjust = ((1000 / (self.tick_interval.as_millis())) + 1) as u16;

        self.client_tick_adjust = ((3000 / (self.tick_interval.as_millis())) + 1) as u16;
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
        self.server_tick_adjust =
            ((((jitter_deviation * 3.0) / 2.0) / self.tick_interval.as_millis() as f32) + 1.0)
                .ceil() as u16;
        self.client_tick_adjust = (((rtt_average + (jitter_deviation * 3.0) / 2.0)
            / (self.tick_interval.as_millis() as f32))
            + 1.0)
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
