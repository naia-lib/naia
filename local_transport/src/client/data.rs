use std::{collections::VecDeque, sync::{Arc, Mutex}};

use crate::shared::{ClientRecvError, ClientSendError, ClientServerAddr};
use super::addr_cell::LocalAddrCell;

// Client packet sender
#[derive(Clone)]
pub struct LocalClientSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
}

impl LocalClientSender {
    pub(crate) fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, addr_cell: LocalAddrCell) -> Self {
        Self { queue, addr_cell }
    }

    pub fn send(&self, payload: &[u8]) -> Result<(), ClientSendError> {
        // Check if server address is known before sending
        match self.addr_cell.get() {
            ClientServerAddr::Finding => {
                return Err(ClientSendError);
            }
            ClientServerAddr::Found(_) => {}
        }
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(payload.to_vec());
        log::trace!("[LocalTransport] Client sent {} bytes", payload.len());
        Ok(())
    }

    pub fn server_addr(&self) -> ClientServerAddr {
        self.addr_cell.get()
    }
}

// Client packet receiver
#[derive(Clone)]
pub struct LocalClientReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalClientReceiver {
    pub(crate) fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, addr_cell: LocalAddrCell) -> Self {
        Self {
            queue,
            addr_cell,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<&[u8]>, ClientRecvError> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(payload) = queue.pop_front() {
            log::trace!("[LocalTransport] Client received {} bytes", payload.len());
            let boxed = payload.into_boxed_slice();
            *self.last_payload.lock().unwrap() = Some(boxed);
            let payload_ref = self.last_payload.lock().unwrap();
            let payload_slice = payload_ref.as_ref().unwrap().as_ref();
            let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice) };
            Ok(Some(static_ref))
        } else {
            Ok(None)
        }
    }

    pub fn server_addr(&self) -> ClientServerAddr {
        self.addr_cell.get()
    }
}

