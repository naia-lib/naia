use std::{net::SocketAddr, sync::{Arc, Mutex}};

use tokio::sync::mpsc;

use crate::shared::{ServerRecvError, ServerSendError};

// Server packet sender
#[derive(Clone)]
pub struct LocalServerSender {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    client_addr: SocketAddr,
}

impl LocalServerSender {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Vec<u8>>, client_addr: SocketAddr) -> Self {
        Self { tx, client_addr }
    }

    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ServerSendError> {
        if address != &self.client_addr {
            return Err(ServerSendError);
        }
        // Send via unbounded channel (non-blocking)
        self.tx.send(payload.to_vec())
            .map_err(|_| ServerSendError)?;
        log::trace!("[LocalTransport] Server sent {} bytes", payload.len());
        Ok(())
    }
}

// Server packet receiver
#[derive(Clone)]
pub struct LocalServerReceiver {
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    client_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalServerReceiver {
    pub(crate) fn new(rx: mpsc::UnboundedReceiver<Vec<u8>>, client_addr: SocketAddr) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
            client_addr,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        // Try to receive from channel (non-blocking)
        let mut rx_guard = self.rx.lock().unwrap();
        if let Ok(payload) = rx_guard.try_recv() {
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

