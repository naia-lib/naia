use naia_shared::{BitReader, BitWriter, PingIndex, PingStore, Serde, Timer};

use crate::connection::time_config::TimeConfig;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct TimeManager {
    ping_timer: Timer,
    sent_pings: PingStore,
    pub rtt: f32,
    pub jitter: f32,
    rtt_smoothing_factor: f32,
    rtt_smoothing_factor_inv: f32,
}

impl TimeManager {
    pub fn new(ping_config: &TimeConfig) -> Self {
        let rtt_average = ping_config.rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = ping_config.jitter_initial_estimate.as_secs_f32() * 1000.0;

        TimeManager {
            ping_timer: Timer::new(ping_config.ping_interval),
            sent_pings: PingStore::new(),
            rtt: rtt_average,
            jitter: jitter_average,
            rtt_smoothing_factor: ping_config.rtt_smoothing_factor,
            rtt_smoothing_factor_inv: 1.0 - ping_config.rtt_smoothing_factor,
        }
    }

    /// Returns whether a ping message should be sent
    pub fn should_send_ping(&self) -> bool {
        self.ping_timer.ringing()
    }

    /// Get an outgoing ping payload
    pub fn write_ping(&mut self, writer: &mut BitWriter) {
        self.ping_timer.reset();

        let ping_index = self.sent_pings.push_new();

        // write index
        ping_index.ser(writer);
    }

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, reader: &mut BitReader) {
        if let Ok(ping_index) = PingIndex::de(reader) {
            match self.sent_pings.remove(ping_index) {
                None => {}
                Some(ping_instant) => {
                    let rtt_millis = &ping_instant.elapsed().as_secs_f32() * 1000.0;
                    self.process_new_rtt(rtt_millis);
                }
            }
        }
    }

    /// Recompute rtt/jitter estimations
    fn process_new_rtt(&mut self, rtt_millis: f32) {
        let new_jitter = ((rtt_millis - self.rtt) / 2.0).abs();
        self.jitter = (self.rtt_smoothing_factor_inv * self.jitter)
            + (self.rtt_smoothing_factor * new_jitter);

        self.rtt =
            (self.rtt_smoothing_factor_inv * self.rtt) + (self.rtt_smoothing_factor * rtt_millis);
    }
}
