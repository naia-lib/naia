use std::time::Duration;

use crate::Timer;

#[derive(Debug)]
pub struct PingManager {
    ping_timer: Timer,
}

impl PingManager {
    pub fn new(ping_interval: Duration) -> Self {
        PingManager {
            ping_timer: Timer::new(ping_interval),
        }
    }

    /// Returns whether a ping message should be sent
    pub fn should_send_ping(&self) -> bool {
        return self.ping_timer.ringing();
    }

    /// Get an outgoing ping payload
    pub fn get_ping_payload(&self) -> Box<[u8]> {
        unimplemented!()
    }

    /// Process an incoming ping payload
    pub fn process_ping(&self, ping_payload: &[u8]) -> Box<[u8]> {
        unimplemented!()
    }
}
