
use std::time::Duration;

use crate::Timer;

pub struct ConnectionManager {
    heartbeat_timer: Timer,
    timeout_timer: Timer,
}

impl ConnectionManager {
    pub fn new(heartbeat_duration: Duration, timeout_duration: Duration) -> Self {
        ConnectionManager {
            heartbeat_timer: Timer::new(heartbeat_duration),
            timeout_timer: Timer::new(timeout_duration),
        }
    }

    pub fn mark_sent(&mut self) {
        self.heartbeat_timer.reset();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        self.heartbeat_timer.ringing()
    }

    pub fn mark_heard(&mut self) {
        self.timeout_timer.reset();
    }

    pub fn should_drop(&self) -> bool {
        self.timeout_timer.ringing()
    }
}
