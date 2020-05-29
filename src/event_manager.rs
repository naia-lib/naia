
use log::{info};

pub struct EventManager {

}

impl EventManager {
    pub fn new() -> Self {
        EventManager {

        }
    }

    pub fn notify_packet_delivered(&self, packet_index: u16) {
        info!("yay event manager notified DELIVERED!");
    }

    pub fn notify_packet_dropped(&self, packet_index: u16) {
        info!("yay event manager notified DROPPED!");
    }
}