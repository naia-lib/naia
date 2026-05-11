use naia_shared::{BitReader, BitWriter, PingIndex, PingStore, Serde, Timer};

use crate::{connection::ping_config::PingConfig, time_manager::TimeManager};

const RTT_RING_SIZE: usize = 32;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct PingManager {
    pub rtt_average: f32,
    pub jitter_average: f32,
    ping_timer: Timer,
    sent_pings: PingStore,
    // 32-sample ring buffer for p50/p99 estimation (64 bytes total).
    rtt_ring: [u16; RTT_RING_SIZE],
    rtt_ring_pos: usize,
    rtt_ring_count: usize,
}

impl PingManager {
    pub fn new(ping_config: &PingConfig) -> Self {
        let rtt_average = ping_config.rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = ping_config.jitter_initial_estimate.as_secs_f32() * 1000.0;

        Self {
            rtt_average,
            jitter_average,
            ping_timer: Timer::new(ping_config.ping_interval),
            sent_pings: PingStore::new(),
            rtt_ring: [0u16; RTT_RING_SIZE],
            rtt_ring_pos: 0,
            rtt_ring_count: 0,
        }
    }

    /// Returns whether a ping message should be sent
    pub fn should_send_ping(&self) -> bool {
        self.ping_timer.ringing()
    }

    /// Get an outgoing ping payload
    pub fn write_ping(&mut self, writer: &mut BitWriter, time_manager: &TimeManager) {
        self.ping_timer.reset();

        let ping_index = self.sent_pings.push_new(time_manager.game_time_now());

        // write index
        ping_index.ser(writer);
    }

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, time_manager: &TimeManager, reader: &mut BitReader) {
        if let Ok(ping_index) = PingIndex::de(reader) {
            match self.sent_pings.remove(ping_index) {
                None => {}
                Some(game_instant) => {
                    let rtt_millis = time_manager.game_time_since(&game_instant).as_millis();
                    self.process_new_rtt(rtt_millis);
                }
            }
        }
    }

    /// 50th-percentile RTT in milliseconds over the last 32 samples.
    /// Returns the EWMA average if fewer than 2 samples have been recorded.
    pub fn rtt_p50_ms(&self) -> f32 {
        self.rtt_percentile(50)
    }

    /// 99th-percentile RTT in milliseconds over the last 32 samples.
    /// Returns the EWMA average if fewer than 2 samples have been recorded.
    pub fn rtt_p99_ms(&self) -> f32 {
        self.rtt_percentile(99)
    }

    fn rtt_percentile(&self, pct: usize) -> f32 {
        let count = self.rtt_ring_count;
        if count < 2 {
            return self.rtt_average;
        }
        let mut sorted = [0u16; RTT_RING_SIZE];
        sorted[..count].copy_from_slice(&self.rtt_ring[..count]);
        sorted[..count].sort_unstable();
        let idx = ((pct * (count - 1)) / 100).min(count - 1);
        sorted[idx] as f32
    }

    /// Recompute rtt/jitter estimations
    fn process_new_rtt(&mut self, rtt_millis: u32) {
        let rtt_millis_f32 = rtt_millis as f32;
        let new_jitter = ((rtt_millis_f32 - self.rtt_average) / 2.0).abs();
        self.jitter_average = (0.9 * self.jitter_average) + (0.1 * new_jitter);
        self.rtt_average = (0.9 * self.rtt_average) + (0.1 * rtt_millis_f32);

        // Update ring buffer (saturate at u16::MAX ≈ 65s, adequate for any real RTT).
        let sample = rtt_millis.min(u16::MAX as u32) as u16;
        if self.rtt_ring_count < RTT_RING_SIZE {
            self.rtt_ring[self.rtt_ring_count] = sample;
            self.rtt_ring_count += 1;
        } else {
            self.rtt_ring[self.rtt_ring_pos] = sample;
            self.rtt_ring_pos = (self.rtt_ring_pos + 1) % RTT_RING_SIZE;
        }
    }
}
