use std::time::Duration;

use naia_shared::{wrapping_diff, sequence_greater_than, Instant, Serde, Tick, sequence_less_than};

use crate::{
    client::{BitReader, BitWriter},
    TickConfig,
};

/// Manages the current tick for the host
pub struct TickManager {
    config: TickConfig,
    /// How much time in milliseconds does a tick last
    tick_interval_millis: f32,
    /// Used to modify the tick interval. A value >1.0 means that the tick interval will be bigger
    tick_speed_factor: f32,
    /// Smoothed measure how fast the tick offset is varying
    tick_offset_speed_avg: f32,
    /// Smoothed measure of how much ahead the client tick is compared to the server tick
    tick_offset_avg: f32,
    /// current client tick
    internal_tick: Tick,
    client_sending_tick_adjust: f32,
    server_receivable_tick_adjust: f32,
    client_receiving_tick_adjust: f32,
    last_tick_instant: Instant,
    interpolation: f32,
    accumulator: f32,
    /// Last tick offset recorded
    last_tick_offset: f32,
    ticks_recorded: u8,
    last_server_tick: Tick,
    inv_tick_offset_smooth_factor: f32,
    received_ticks: Tick,
    last_client_receiving_tick: Tick,
    last_client_sending_tick: Tick,
    last_server_receivable_tick: Tick,
}

impl TickManager {
    /// Create a new TickManager with a given tick interval duration
    pub fn new(tick_interval: Duration, config: TickConfig) -> Self {
        let tick_interval_millis = tick_interval.as_millis() as f32;

        let inv_tick_offset_smooth_factor = 1.0 - config.tick_offset_smooth_factor;

        TickManager {
            config,
            tick_interval_millis,
            tick_speed_factor: 1.0,
            tick_offset_avg: 0.0,
            tick_offset_speed_avg: 0.0,
            internal_tick: 0,
            client_sending_tick_adjust: 0.0,
            client_receiving_tick_adjust: 0.0,
            server_receivable_tick_adjust: 0.0,
            last_tick_instant: Instant::now(),
            accumulator: 0.0,
            interpolation: 0.0,
            last_tick_offset: 0.0,
            ticks_recorded: 0,
            last_server_tick: 0,
            inv_tick_offset_smooth_factor,
            received_ticks: 0,
            last_client_receiving_tick: 0,
            last_client_sending_tick: 0,
            last_server_receivable_tick: 0,
        }
    }

    pub fn write_client_tick(&mut self, writer: &mut BitWriter) -> Tick {
        let client_tick = self.client_sending_tick();
        client_tick.ser(writer);
        client_tick
    }

    /// Read server tick from any packet that includes it and updates
    ///
    /// # Panics
    ///
    /// If the incoming packet from the server doesn't contain the server tick
    pub fn read_server_tick(&mut self, reader: &mut BitReader) -> Tick {
        let server_tick = Tick::de(reader).expect("unable to read server tick from packet");

        self.record_server_tick(server_tick);

        server_tick
    }

    /// This should run every frame. Check if enough time has passed so that we move to the next tick
    /// Also keeps track of internal state such as the interpolation percentage
    pub fn accumulate_ticks(&mut self) {
        let tick_interval_millis = self.tick_interval_millis * self.tick_speed_factor;

        let frame_millis = self.last_tick_instant.elapsed().as_nanos() as f32 / 1000000.0;
        // if frame_seconds > tick_interval_seconds {
        //     info!("big frame delta: {} ms", frame_seconds*1000.0);
        // }
        self.accumulator += frame_millis; //.min(250.0);
        self.last_tick_instant = Instant::now();

        while self.accumulator >= tick_interval_millis {
            self.accumulator -= tick_interval_millis;
            self.internal_tick = self.internal_tick.wrapping_add(1);
            self.received_ticks += 1;
        }
        self.interpolation = self.accumulator / tick_interval_millis;
    }

    pub fn recv_client_ticks(&mut self) -> Tick {
        std::mem::take(&mut self.received_ticks)
    }

    /// Return the current interpolation of the frame between the two surrounding ticks
    /// 0.20 means that we are 20% of the way to the next tick
    pub fn interpolation(&self) -> f32 {
        self.interpolation
    }

    /// Using information from the Server and RTT/Jitter measurements, determine
    /// the appropriate future intended tick
    fn record_server_tick(&mut self, server_tick: Tick) {

        // Init during first received ticks
        if self.ticks_recorded <= 1 {
            if self.ticks_recorded == 1 {
                let mut tick_offset = wrapping_diff(self.internal_tick, server_tick) as f32;
                // add accumulation, which is a fraction measure of the current Tick (0.0 -> 1.0)
                tick_offset -= self.interpolation;
                self.tick_offset_avg = tick_offset;
                self.last_tick_offset = tick_offset;

                self.last_server_tick = server_tick;
                self.last_client_receiving_tick = server_tick;
                self.last_client_sending_tick = server_tick;
                self.last_server_receivable_tick = server_tick;
            }

            self.ticks_recorded += 1;
            return;
        }

        // Make sure we only record server_ticks going FORWARD
        if !sequence_greater_than(server_tick, self.last_server_tick) {
            return;
        }

        self.last_server_tick = server_tick;

        let mut tick_offset = wrapping_diff(self.internal_tick, server_tick) as f32;
        // add accumulation, which is a fraction measure of the current Tick (0.0 -> 1.0)
        tick_offset -= self.interpolation;

        self.tick_offset_avg = (self.inv_tick_offset_smooth_factor * self.tick_offset_avg)
            + (self.config.tick_offset_smooth_factor * tick_offset);

        // Tick Offset Divergence Speed Estimate
        let tick_offset_speed = tick_offset - self.last_tick_offset;
        self.last_tick_offset = tick_offset;

        self.tick_offset_speed_avg = (self.inv_tick_offset_smooth_factor * self.tick_offset_speed_avg)
            + (self.config.tick_offset_smooth_factor * tick_offset_speed);

        // Counteract Divergence Speed by slowing down / speeding up Client Tick
        if self.tick_offset_speed_avg > 1.0 {
            self.tick_speed_factor -= 0.1;
            self.tick_offset_speed_avg = 0.0;
        }
        if self.tick_offset_speed_avg < -1.0 {
            self.tick_speed_factor += 0.1;
            self.tick_offset_speed_avg = 0.0;
        }
    }

    pub fn adjust_network_conditions(&mut self, rtt_average: f32, jitter_average: f32) {
        // Calculate incoming & outgoing jitter buffer tick offsets

        let jitter_limit = jitter_average * 4.0;
        self.client_receiving_tick_adjust = (jitter_limit / self.tick_interval_millis)
            .max(self.config.minimum_recv_jitter_buffer_size as f32);

        // NOTE: I've struggled multiple times with why rtt_average instead of
        // ping_average exists in this calculation, figured it out, then
        // returned to struggle later. This is not a bug!
        // Keep in mind that self.server_tick here is the tick we have RECEIVED from the
        // Server which means that the real current server_tick is likely
        // self.server_tick + ping_average / tick_interval.
        // By using rtt_average here, we are correcting for our late (and
        // lesser) self.server_tick value
        let client_sending_adjust_millis = self
            .config
            .minimum_latency_millis
            .max(rtt_average + jitter_limit);
        self.client_sending_tick_adjust = (client_sending_adjust_millis
            / self.tick_interval_millis)
            .max(self.config.minimum_send_jitter_buffer_size as f32);

        // Calculate estimate of earliest tick Server could receive now
        let server_receivable_adjust_millis = rtt_average - jitter_limit;
        self.server_receivable_tick_adjust =
            server_receivable_adjust_millis / self.tick_interval_millis;
    }

    pub fn server_tick(&self) -> Tick {
        let mut output = self.server_tick_estimate();
        wrap_f32(&mut output);
        output.round() as Tick
    }

    /// Gets the tick at which the Client is sending updates
    pub fn client_sending_tick(&mut self) -> Tick {
        let mut output = self.server_tick_estimate() + self.client_sending_tick_adjust + 2.0;
        wrap_f32(&mut output);
        let output_tick = output.round() as Tick;

        // Ensure this returned Tick is ALWAYS advancing
        if sequence_less_than(output_tick, self.last_client_sending_tick) {
            return self.last_client_sending_tick;
        } else {
            self.last_client_sending_tick = output_tick;
            return output_tick;
        }
    }

    /// Gets the tick at which to receive messages from the Server (after jitter
    /// buffer offset is applied)
    pub fn client_receiving_tick(&mut self) -> Tick {

        let mut output = self.server_tick_estimate() - self.client_receiving_tick_adjust - 1.0;
        wrap_f32(&mut output);
        let output_tick = output.round() as Tick;

        // Ensure this returned Tick is ALWAYS advancing
        if sequence_less_than(output_tick, self.last_client_receiving_tick) {
            return self.last_client_receiving_tick;
        } else {
            self.last_client_receiving_tick = output_tick;
            return output_tick;
        }
    }

    /// Gets the earliest tick the Server may be able to receive Client messages
    pub fn server_receivable_tick(&mut self) -> Tick {

        let mut output = self.server_tick_estimate() + self.server_receivable_tick_adjust;
        wrap_f32(&mut output);
        let output_tick = output.round() as Tick;

        // Ensure this returned Tick is ALWAYS advancing
        if sequence_less_than(output_tick, self.last_server_receivable_tick) {
            return self.last_server_receivable_tick;
        } else {
            self.last_server_receivable_tick = output_tick;
            return output_tick;
        }
    }

    fn server_tick_estimate(&self) -> f32 {
        (self.internal_tick as f32) + self.tick_offset_avg
    }

    pub fn tick_offset_avg(&self) -> f32 { self.tick_offset_avg }
}

fn wrap_f32(val: &mut f32) {
    const U16_MAX_AS_F32: f32 = u16::MAX as f32;
    while *val < 0.0 {
        *val += U16_MAX_AS_F32;
    }
    while *val > U16_MAX_AS_F32 {
        *val -= U16_MAX_AS_F32;
    }
}
