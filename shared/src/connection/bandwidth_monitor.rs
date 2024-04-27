use std::time::Duration;

pub struct BandwidthMonitor {
    time_queue: ExpiringTimeQueue<usize>,
    total_bytes: u16,
    to_kbps_factor: f32,
}

impl BandwidthMonitor {
    pub fn new(bandwidth_measure_duration: Duration) -> Self {
        Self {
            time_queue: ExpiringTimeQueue::new(bandwidth_measure_duration),
            total_bytes: 0,
            to_kbps_factor: 0.008 / bandwidth_measure_duration.as_secs_f32(),
        }
    }

    pub fn record_packet(&mut self, bytes: usize) {
        self.clear_expired_packets();

        self.total_bytes += bytes as u16;
        self.time_queue.add_item(bytes);
    }

    pub fn bandwidth(&mut self) -> f32 {
        self.clear_expired_packets();

        self.total_bytes as f32 * self.to_kbps_factor
    }

    fn clear_expired_packets(&mut self) {
        let now = Instant::now();
        while let Some(bytes) = self.time_queue.pop_item(&now) {
            self.total_bytes -= bytes as u16;
        }
    }
}

////

use naia_socket_shared::{Instant, TimeQueue};

#[derive(Clone)]
struct ExpiringTimeQueue<T: Eq + PartialEq> {
    queue: TimeQueue<T>,
    expire_time: Duration,
}

impl<T: Eq + PartialEq> ExpiringTimeQueue<T> {
    pub fn new(duration: Duration) -> Self {
        Self {
            queue: TimeQueue::new(),
            expire_time: duration,
        }
    }

    pub fn add_item(&mut self, item: T) {
        let mut instant = Instant::now();
        instant.add_millis(self.expire_time.as_millis() as u32);
        self.queue.add_item(instant, item);
    }

    pub fn pop_item(&mut self, now: &Instant) -> Option<T> {
        self.queue.pop_item(now)
    }
}
