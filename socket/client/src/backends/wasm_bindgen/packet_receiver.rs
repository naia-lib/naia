use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError},
};

use crate::{error::NaiaClientSocketError, server_addr::ServerAddr};

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PlainPacketReceiver {
    message_queue: Arc<Mutex<VecDeque<Box<[u8]>>>>,
    server_addr: AddrCell,
    last_payload: Option<Box<[u8]>>,
}

impl PlainPacketReceiver {
    /// Create a new PacketReceiver, if supplied with the RtcDataChannel and a
    /// reference to a list of dropped messages
    pub fn new(data_port: &DataPort, addr_cell: &AddrCell) -> Self {
        Self {
            message_queue: data_port.message_queue(),
            server_addr: addr_cell.clone(),
            last_payload: None,
        }
    }
}

impl PlainPacketReceiver {
    /// Pops and returns the next queued packet, or `None` if none is waiting.
    pub fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        match self
            .message_queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
        {
            Some(payload) => {
                let payload = self.last_payload.insert(payload);
                Ok(Some(&payload[..]))
            }
            None => Ok(None),
        }
    }

    /// Get the Server's Socket address
    pub fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }
}
