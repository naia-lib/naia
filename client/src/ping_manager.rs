use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_shared::{Instant, PacketReader, SequenceBuffer, SequenceNumber, Timer};

use naia_client_socket::Packet;

#[derive(Clone)]
struct SentPing {
    time_sent: Instant,
}

pub struct PingManager {
    ping_timer: Timer,
    sent_pings: SequenceBuffer<SentPing>,
    ping_index: SequenceNumber,
    rtt_average: f32,
    rtt_deviation: f32,
    rtt_smoothing_factor: f32,
    rtt_smoothing_factor_inv: f32,
}

impl PingManager {
    pub fn new(ping_interval: Duration, rtt_initial_estimate: Duration, jitter_initial_estimate: Duration, rtt_smoothing_factor: f32) -> Self {

        let rtt_average = rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = jitter_initial_estimate.as_secs_f32() * 1000.0;

        PingManager {
            ping_index: 0,
            ping_timer: Timer::new(ping_interval),
            sent_pings: SequenceBuffer::with_capacity(100),
            rtt_average,
            rtt_deviation: jitter_average,
            rtt_smoothing_factor,
            rtt_smoothing_factor_inv: 1.0 - rtt_smoothing_factor,
        }
    }

    /// Returns whether a ping message should be sent
    pub fn should_send_ping(&self) -> bool {
        self.ping_timer.ringing()
    }

    /// Get an outgoing ping payload
    pub fn ping_packet(&mut self) -> Packet {
        self.ping_timer.reset();

        self.sent_pings.insert(
            self.ping_index,
            SentPing {
                time_sent: Instant::now(),
            },
        );

        let mut out_bytes = Vec::<u8>::new();
        out_bytes.write_u16::<BigEndian>(self.ping_index).unwrap(); // write index

        // increment ping index
        self.ping_index = self.ping_index.wrapping_add(1);

        Packet::new(out_bytes)
    }

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, pong_payload: &[u8]) {
        let mut reader = PacketReader::new(&pong_payload);
        let ping_index = reader.cursor().read_u16::<BigEndian>().unwrap();

        match self.sent_pings.remove(ping_index) {
            None => {}
            Some(ping) => {
                let rtt_millis = &ping.time_sent.elapsed().as_secs_f32() * 1000.0;
                self.process_new_rtt(rtt_millis);
            }
        }
    }

    fn process_new_rtt(&mut self, ping_millis: f32) {
        let old_rtt_avg = self.rtt_average;
        self.rtt_average = (self.rtt_smoothing_factor_inv * old_rtt_avg) + (self.rtt_smoothing_factor * ping_millis);
        self.rtt_deviation = (self.rtt_smoothing_factor_inv * self.rtt_deviation) + (self.rtt_smoothing_factor * (ping_millis - old_rtt_avg).abs());
    }

    /// Gets the current calculated average Round Trip Time to the remote host,
    /// in milliseconds
    pub fn rtt(&self) -> f32 {
        return self.rtt_average;
    }

    /// Gets the current calculated standard deviation of Jitter to the remote
    /// host, in milliseconds
    pub fn jitter(&self) -> f32 {
        return self.rtt_deviation / 2.0;
    }
}
