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

    pub fn send_packet(&self, address: &SocketAddr, bytes: usize) {
        todo!()
    }

    pub fn receive_packet(&self, address: &SocketAddr, bytes: usize) {
        todo!()
    }

    pub fn upload_bandwidth_total(&self) -> f32 {
        todo!()
    }

    pub fn download_bandwidth_total(&self) -> f32 {
        todo!()
    }

    pub fn upload_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        todo!()
    }

    pub fn download_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        todo!()
    }
}