use std::{collections::VecDeque, net::SocketAddr, sync::{Arc, Mutex}};

use crate::shared::{ServerRecvError, ServerSendError};

// Server packet sender
#[derive(Clone)]
pub struct LocalServerSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
}

impl LocalServerSender {
    pub(crate) fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, client_addr: SocketAddr) -> Self {
        Self { queue, client_addr }
    }

    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ServerSendError> {
        if address != &self.client_addr {
            return Err(ServerSendError);
        }
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(payload.to_vec());
        log::trace!("[LocalTransport] Server sent {} bytes", payload.len());
        Ok(())
    }
}

// Server packet receiver
#[derive(Clone)]
pub struct LocalServerReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalServerReceiver {
    pub(crate) fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, client_addr: SocketAddr) -> Self {
        Self {
            queue,
            client_addr,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(payload) = queue.pop_front() {
            log::trace!("[LocalTransport] Server received {} bytes", payload.len());
            let boxed = payload.into_boxed_slice();
            *self.last_payload.lock().unwrap() = Some(boxed);
            let payload_ref = self.last_payload.lock().unwrap();
            let payload_slice = payload_ref.as_ref().unwrap().as_ref();
            let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice) };
            Ok(Some((self.client_addr, static_ref)))
        } else {
            Ok(None)
        }
    }
}

