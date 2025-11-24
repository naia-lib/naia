use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use naia_shared::transport::local::{ClientRecvError, ClientSendError, ClientServerAddr};

use super::addr_cell::LocalAddrCell;

// Client packet sender
#[derive(Clone)]
pub struct LocalClientSender {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    addr_cell: LocalAddrCell,
}

impl LocalClientSender {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Vec<u8>>, addr_cell: LocalAddrCell) -> Self {
        Self { tx, addr_cell }
    }

    pub fn send(&self, payload: &[u8]) -> Result<(), ClientSendError> {
        // Check if server address is known before sending
        match self.addr_cell.get() {
            ClientServerAddr::Finding => {
                return Err(ClientSendError);
            }
            ClientServerAddr::Found(_addr) => {}
        }
        // Send via unbounded channel (non-blocking)
        self.tx.send(payload.to_vec())
            .map_err(|_| ClientSendError)?;
        Ok(())
    }

    pub fn server_addr(&self) -> ClientServerAddr {
        self.addr_cell.get()
    }
}

// Client packet receiver
#[derive(Clone)]
pub struct LocalClientReceiver {
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalClientReceiver {
    pub(crate) fn new(rx: mpsc::UnboundedReceiver<Vec<u8>>, addr_cell: LocalAddrCell) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
            addr_cell,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<&[u8]>, ClientRecvError> {
        // Try to receive from channel (non-blocking)
        let mut rx_guard = self.rx.lock().unwrap();
        if let Ok(payload) = rx_guard.try_recv() {
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

