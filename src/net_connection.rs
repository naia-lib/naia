
use std::time::Duration;

use crate::{Timer, PacketType};

use super::{
    sequence_buffer::{SequenceNumber},
    Timestamp,
    ack_manager::AckManager,
    event_manager::EventManager,
    ghost_manager::GhostManager
};

pub struct NetConnection {
    pub connection_timestamp: Timestamp,
    heartbeat_manager: Timer,
    timeout_manager: Timer,
    ack_manager: AckManager,
    event_manager: EventManager,
    ghost_manager: GhostManager,
}

impl NetConnection {
    pub fn new(heartbeat_interval: Duration, timeout_duration: Duration, host_name: &str, connection_timestamp: Timestamp) -> Self {
        NetConnection {
            connection_timestamp,
            heartbeat_manager: Timer::new(heartbeat_interval),
            timeout_manager: Timer::new(timeout_duration),
            ack_manager: AckManager::new(host_name),
            event_manager: EventManager::new(),
            ghost_manager: GhostManager::new(),
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

    pub fn process_incoming(&mut self, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_incoming(&self.event_manager, &self.ghost_manager, payload)
    }

    pub fn process_outgoing(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
        self.ack_manager.process_outgoing(packet_type, payload)
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        self.ack_manager.local_sequence_num()
    }
}