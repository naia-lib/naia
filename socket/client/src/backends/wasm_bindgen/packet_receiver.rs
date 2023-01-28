use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    error::NaiaClientSocketError, packet_receiver::PacketReceiverTrait, server_addr::ServerAddr,
};

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles receiving messages from the Server through a given Client Socket
pub struct PacketReceiverImpl {
    message_queue: Arc<Mutex<VecDeque<Box<[u8]>>>>,
    server_addr: AddrCell,
    last_payload: Option<Box<[u8]>>,
}

impl PacketReceiverImpl {
    /// Create a new PacketReceiver, if supplied with the RtcDataChannel and a
    /// reference to a list of dropped messages
    pub fn new(data_port: &DataPort, addr_cell: &AddrCell) -> Self {
        PacketReceiverImpl {
            message_queue: data_port.message_queue(),
            server_addr: addr_cell.clone(),
            last_payload: None,
        }
    }
}

impl PacketReceiverTrait for PacketReceiverImpl {
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        match self
            .message_queue
            .lock()
            .expect("This should never happen, message_queue should always be available in a single-threaded context")
            .pop_front()
        {
            Some(payload) => {
                self.last_payload = Some(payload);
                Ok(Some(self.last_payload.as_ref().unwrap()))
            }
            None => Ok(None),
        }
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        self.server_addr.get()
    }
}
