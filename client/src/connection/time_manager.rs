use naia_shared::{BitReader, BitWriter, PingIndex, PingStore, Serde, Tick, Timer};
use std::time::Duration;

use crate::connection::time_config::TimeConfig;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct TimeManager {
    pub rtt: f32,
    pub jitter: f32,
    pub tick_duration: f32,
    ping_timer: Timer,
    sent_pings: PingStore,
}

impl TimeManager {
    pub(crate) fn server_receivable_tick(&self) -> Tick {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn read_server_tick(&self, reader: &mut BitReader) -> Tick {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn write_client_tick(&self, writer: &mut BitWriter) -> Tick {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn interpolation(&self) -> f32 {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn client_sending_tick(&self) -> Tick {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn client_receiving_tick(&self) -> Tick {
        todo!()
    }
}

impl TimeManager {
    pub(crate) fn recv_client_tick(&self) -> bool {
        todo!()
    }
}

impl TimeManager {
    pub fn new(time_config: &TimeConfig, tick_duration: &Duration) -> Self {
        let rtt_average = time_config.rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = time_config.jitter_initial_estimate.as_secs_f32() * 1000.0;
        let tick_duration_average = tick_duration.as_secs_f32() * 1000.0;

        TimeManager {
            rtt: rtt_average,
            jitter: jitter_average,
            tick_duration: tick_duration_average,
            ping_timer: Timer::new(time_config.ping_interval),
            sent_pings: PingStore::new(),
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

        self.jitter = (0.9 * self.jitter) + (0.1 * new_jitter);
        self.rtt = (0.9 * self.rtt) + (0.1 * rtt_millis);
    }
}
