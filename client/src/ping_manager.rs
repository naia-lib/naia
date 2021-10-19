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
    rtt_average: f32,
    rtt_variance: f32,
    rtt_deviation: f32,
}

impl PingManager {
    pub fn new(ping_interval: Duration, rtt_sample_size: u16) -> Self {
        PingManager {
            ping_index: 0,
            ping_timer: Timer::new(ping_interval),
            sent_pings: SequenceBuffer::with_capacity(rtt_sample_size),
            samples: 0.0,
            max_samples: f32::from(rtt_sample_size),
            rtt_average: 0.0,
            rtt_variance: 0.0,
            rtt_deviation: 0.0,
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
        let ping_index = reader.get_cursor().read_u16::<BigEndian>().unwrap();

        match self.sent_pings.remove(ping_index) {
            None => {}
            Some(ping) => {
                self.process_new_rtt(&ping.time_sent.elapsed().as_secs_f32() * 1000.0);
            }
        }
    }

    fn process_new_rtt(&mut self, elapsed_millis: f32) {
        if self.samples == 0.0 {
            self.rtt_average = elapsed_millis;
            self.samples = 1.0;
            return;
        } else {
            if self.samples < self.max_samples {
                self.samples += 1.0;
            }

            self.rtt_average =
                ((self.rtt_average * self.samples) + elapsed_millis) / (self.samples + 1.0);

            let new_variance = (elapsed_millis - self.rtt_average).powi(2);
            self.rtt_variance =
                ((self.rtt_variance * self.samples) + new_variance) / (self.samples + 1.0);

            self.rtt_deviation = self.rtt_variance.sqrt();
        }
    }

    /// Gets the current calculated average Round Trip Time to the remote host,
    /// in milliseconds
    pub fn get_rtt(&self) -> f32 {
        return self.rtt_average;
    }

    /// Gets the current calculated standard deviation of Jitter to the remote
    /// host, in milliseconds
    pub fn get_jitter(&self) -> f32 {
        return self.rtt_deviation;
    }
}
