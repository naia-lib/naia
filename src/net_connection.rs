
use std::time::Duration;

use crate::{Timer, AckManager};
use super::StandardHeader;
use super::Timestamp;

pub struct NetConnection {
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    pub ack_manager: AckManager,
    pub connection_timestamp: Timestamp,
}

impl NetConnection {
    pub fn new(heartbeat_interval: Duration, timeout_duration: Duration, host_name: &str, connection_timestamp: Timestamp) -> Self {
        NetConnection {
            heartbeat_manager: Timer::new(heartbeat_interval),
            timeout_manager: Timer::new(timeout_duration),
            ack_manager: AckManager::new(host_name),
            connection_timestamp,
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

    pub fn get_headerless_payload(payload: &[u8]) -> Box<[u8]> {
        let (_, stripped_message) = StandardHeader::read(payload);
        stripped_message
    }
}