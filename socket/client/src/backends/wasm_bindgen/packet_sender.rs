use std::collections::VecDeque;

use web_sys::{RtcDataChannel, RtcDataChannelState};

use crate::server_addr::ServerAddr;

use super::addr_cell::AddrCell;

/// Handles sending messages to the Server for a given Client Socket
#[derive(Clone)]
pub struct PacketSender {
    data_channel: RtcDataChannel,
    server_addr: AddrCell,
}

impl PacketSender {
    /// Create a new PacketSender, if supplied with the RtcDataChannel and a
    /// reference to a list of dropped messages
    pub fn new(data_channel: RtcDataChannel, server_addr: AddrCell) -> Self {
        PacketSender {
            data_channel,
            server_addr,
        }
    }

    /// Send a Packet to the Server
    pub fn send(&self, payload: &[u8]) {
        if self.data_channel.ready_state() == RtcDataChannelState::Open {
            self.data_channel.send_with_u8_array(payload).unwrap();
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }
}

unsafe impl Send for PacketSender {}
unsafe impl Sync for PacketSender {}
