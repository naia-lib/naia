
use log::{info};

pub struct GhostManager {

}

impl GhostManager {
    pub fn new() -> Self {
        GhostManager {

        }
    }

    pub fn notify_packet_delivered(&self, packet_index: u16) {
        info!("yay ghost manager notified DELIVERED!");
    }

    pub fn notify_packet_dropped(&self, packet_index: u16) {
        info!("yay ghost manager notified DROPPED!");
    }
}