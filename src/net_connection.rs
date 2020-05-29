
use std::time::Duration;

use crate::{Timer, AckManager};

pub struct NetConnection {
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    pub ack_manager: AckManager,
}

impl NetConnection {
    pub fn new(heartbeat_interval: Duration, timeout_duration: Duration, host_name: &str) -> Self {
        NetConnection {
            heartbeat_manager: Timer::new(heartbeat_interval),
            timeout_manager: Timer::new(timeout_duration),
            ack_manager: AckManager::new(host_name),
        }
    }

    pub fn mark_sent(&mut self) {
        self.heartbeat_manager.reset();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        self.heartbeat_manager.ringing()
    }

    pub fn mark_heard(&mut self) {
        self.timeout_manager.reset();
    }

    pub fn should_drop(&self) -> bool {
        self.timeout_manager.ringing()
    }
}