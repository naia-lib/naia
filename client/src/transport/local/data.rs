use std::sync::Arc;

use parking_lot::Mutex;

use std::sync::mpsc;

use log::debug;
use naia_shared::transport::local::{ClientRecvError, ClientSendError, ClientServerAddr};

use super::addr_cell::LocalAddrCell;

// Client packet sender
#[derive(Clone)]
pub struct LocalClientSender {
    tx: mpsc::Sender<Vec<u8>>,
    addr_cell: LocalAddrCell,
}

impl LocalClientSender {
    pub(crate) fn new(tx: mpsc::Sender<Vec<u8>>, addr_cell: LocalAddrCell) -> Self {
        Self { tx, addr_cell }
    }

    pub fn send(&self, payload: &[u8]) -> Result<(), ClientSendError> {
        match self.addr_cell.get() {
            ClientServerAddr::Finding => {
                return Err(ClientSendError);
            }
            ClientServerAddr::Found(_addr) => {}
        }
        self.tx
            .send(payload.to_vec())
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
    rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
    // Holds the most-recently-received packet so receive() can return a &[u8] tied
    // to &self rather than to a now-dropped MutexGuard.
    last_payload: Option<Box<[u8]>>,
}

impl LocalClientReceiver {
    pub(crate) fn new(rx: mpsc::Receiver<Vec<u8>>, addr_cell: LocalAddrCell) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
            addr_cell,
            last_payload: None,
        }
    }

    pub fn receive(&mut self) -> Result<Option<&[u8]>, ClientRecvError> {
        let rx_guard = self.rx.lock();
        match rx_guard.try_recv() {
            Ok(payload) => {
                debug!("[CLIENT_RX] Received packet: {} bytes", payload.len());
                self.last_payload = Some(payload.into_boxed_slice());
                Ok(Some(self.last_payload.as_ref().unwrap().as_ref()))
            }
            Err(_) => {
                self.last_payload = None;
                Ok(None)
            }
        }
    }

    pub fn server_addr(&self) -> ClientServerAddr {
        self.addr_cell.get()
    }
}
