use std::time::Duration;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use naia_shared::{Instant, PacketReader, SequenceBuffer, SequenceNumber, Timer};

#[derive(Clone)]
struct SentPing {
    time_sent: Instant,
}

pub struct PingManager {
    ping_timer: Timer,
    sent_pings: SequenceBuffer<SentPing>,
    ping_index: SequenceNumber,
    samples: f32,
    max_samples: f32,
    ping_average: f32,
    ping_variance: f32,
    ping_deviation: f32,
}

impl PingManager {
    pub fn new(ping_interval: Duration, ping_sample_size: u16) -> Self {
        PingManager {
            ping_index: 0,
            ping_timer: Timer::new(ping_interval),
            sent_pings: SequenceBuffer::with_capacity(ping_sample_size),
            samples: 0.0,
            max_samples: f32::from(ping_sample_size),
            ping_average: 0.0,
            ping_variance: 0.0,
            ping_deviation: 0.0,
        }
    }

    /// Returns whether a ping message should be sent
    pub fn should_send_ping(&self) -> bool {
        self.ping_timer.ringing()
    }

    /// Get an outgoing ping payload
    pub fn get_ping_payload(&mut self) -> Box<[u8]> {
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

        out_bytes.into_boxed_slice()
    }

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, pong_payload: &[u8]) {
        let mut reader = PacketReader::new(&pong_payload);
        let ping_index = reader.cursor().read_u16::<BigEndian>().unwrap();

        match self.sent_pings.remove(ping_index) {
            None => {}
            Some(ping) => {
                let rtt_millis = &ping.time_sent.elapsed().as_secs_f32() * 1000.0;
                let ping_millis = rtt_millis / 2.0;
                self.process_new_ping(ping_millis);
            }
        }
    }

    fn process_new_ping(&mut self, ping_millis: f32) {
        if self.samples == 0.0 {
            self.ping_average = ping_millis;
            self.samples = 1.0;
            return;
        } else {
            self.ping_average =
                ((self.ping_average * self.samples) + ping_millis) / (self.samples + 1.0);

            let new_variance = (ping_millis - self.ping_average).powi(2);
            self.ping_variance =
                ((self.ping_variance * self.samples) + new_variance) / (self.samples + 1.0);

            self.ping_deviation = self.ping_variance.sqrt();

            if self.samples < self.max_samples {
                self.samples += 1.0;
            }
        }
    }

    /// Gets the current calculated average Round Trip Time to the remote host,
    /// in milliseconds
    pub fn get_rtt(&self) -> f32 {
        return self.ping_average * 2.0;
    }

    /// Gets the current calculated average Round Trip Time to the remote host,
    /// in milliseconds
    pub fn get_ping(&self) -> f32 {
        return self.ping_average;
    }

    /// Gets the current calculated standard deviation of Jitter to the remote
    /// host, in milliseconds
    pub fn get_jitter(&self) -> f32 {
        return self.ping_deviation;
    }
}
