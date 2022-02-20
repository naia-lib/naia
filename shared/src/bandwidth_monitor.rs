use std::{net::SocketAddr, time::Duration};

pub struct BandwidthMonitor {
    bandwidth_measure_duration: Duration
}

impl BandwidthMonitor {
    pub fn new(bandwidth_measure_duration: Duration) -> Self {
        BandwidthMonitor {
            bandwidth_measure_duration
        }
    }

    pub fn record_packet(&mut self, bytes: usize) {
        todo!()
    }

    pub fn bandwidth(&self) -> f32 {
        todo!()
    }
}