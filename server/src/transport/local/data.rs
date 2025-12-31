use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use naia_shared::transport::local::{LocalTransportHub, ServerRecvError, ServerSendError};

// Server packet sender (always uses hub-based multiplexing)
#[derive(Clone)]
pub struct LocalServerSender {
    hub: LocalTransportHub,
}

impl LocalServerSender {
    pub fn new(hub: LocalTransportHub) -> Self {
        Self { hub }
    }

    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ServerSendError> {
        self.hub
            .send_data(address, payload.to_vec())
            .map_err(|_| ServerSendError)?;
        Ok(())
    }
}

// Server packet receiver (always uses hub-based multiplexing)
#[derive(Clone)]
pub struct LocalServerReceiver {
    hub: LocalTransportHub,
    last_payload: Arc<Mutex<Option<(SocketAddr, Box<[u8]>)>>>,
}

impl LocalServerReceiver {
    pub fn new(hub: LocalTransportHub) -> Self {
        Self {
            hub,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        if let Some((client_addr, bytes)) = self.hub.try_recv_data() {
            let boxed = bytes.into_boxed_slice();
            *self.last_payload.lock().unwrap() = Some((client_addr, boxed));
            let payload_ref = self.last_payload.lock().unwrap();
            let (addr, payload_slice) = payload_ref.as_ref().unwrap();
            let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice.as_ref()) };
            Ok(Some((*addr, static_ref)))
        } else {
            Ok(None)
        }
    }
}
