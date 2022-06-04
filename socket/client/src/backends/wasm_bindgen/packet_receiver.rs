use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{
    error::NaiaClientSocketError, packet_receiver::PacketReceiverTrait, server_addr::ServerAddr,
};

use super::{addr_cell::AddrCell, data_port::DataPort};

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PacketReceiverImpl {
    message_queue: Rc<RefCell<VecDeque<Box<[u8]>>>>,
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
            .try_borrow_mut()
            .expect("can't borrow 'message_queue' buffer!")
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

unsafe impl Send for PacketReceiverImpl {}
unsafe impl Sync for PacketReceiverImpl {}
