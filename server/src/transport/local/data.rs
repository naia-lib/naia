use std::net::SocketAddr;

use naia_shared::transport::local::{LocalTransportHub, ServerRecvError, ServerSendError};

// Server packet sender (always uses hub-based multiplexing)
#[doc(hidden)]
#[derive(Clone)]
pub struct LocalServerSender {
    hub: LocalTransportHub,
}

impl LocalServerSender {
    #[doc(hidden)]
    pub fn new(hub: LocalTransportHub) -> Self {
        Self { hub }
    }

    #[doc(hidden)]
    pub fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ServerSendError> {
        self.hub
            .send_data(address, payload.to_vec())
            .map_err(|_| ServerSendError)?;
        Ok(())
    }
}

// Server packet receiver (always uses hub-based multiplexing)
#[doc(hidden)]
#[derive(Clone)]
pub struct LocalServerReceiver {
    hub: LocalTransportHub,
    // Holds the most-recently-received packet so receive() can return a &[u8] tied
    // to &self rather than to a now-dropped MutexGuard.
    last_payload: Option<(SocketAddr, Box<[u8]>)>,
}

impl LocalServerReceiver {
    #[doc(hidden)]
    pub fn new(hub: LocalTransportHub) -> Self {
        Self {
            hub,
            last_payload: None,
        }
    }

    #[doc(hidden)]
    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        if let Some((client_addr, bytes)) = self.hub.try_recv_data() {
            let (addr, payload) = &*self
                .last_payload
                .insert((client_addr, bytes.into_boxed_slice()));
            Ok(Some((*addr, &payload[..])))
        } else {
            self.last_payload = None;
            Ok(None)
        }
    }
}
