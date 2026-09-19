//! Fan-in server transport: serve one naia `Server` from several transports
//! at once (naia-lib/naia#207 — e.g. UDP for standalone plus WebRTC for
//! Wasm behind a single `server.listen(multi)` call).

use std::net::SocketAddr;

use naia_shared::{IdentityToken, ProtocolId};

use super::{
    AuthReceiver, AuthSender, ListenResult, PacketReadiness, PacketReceiver, PacketSender,
    RecvError, SendError, Socket,
};

/// Serves one [`Socket`] endpoint from several transports at once.
///
/// Each inner transport keeps its own bind addresses; client address spaces
/// are expected to be disjoint across transports (a UDP peer address never
/// equals a WebRTC session address), so fan-out delivers to the first
/// transport that accepts the address and fan-in yields each packet once.
pub struct MultiSocket {
    sockets: Vec<Box<dyn Socket>>,
}

impl MultiSocket {
    /// Combine transports into one server endpoint. Panics when empty — an
    /// address-less server could neither receive nor send, so fail loud.
    pub fn new(sockets: Vec<Box<dyn Socket>>) -> Self {
        assert!(
            !sockets.is_empty(),
            "MultiSocket needs at least one transport"
        );
        Self { sockets }
    }
}

impl From<MultiSocket> for Box<dyn Socket> {
    fn from(val: MultiSocket) -> Self {
        Box::new(val)
    }
}

impl Socket for MultiSocket {
    fn listen(self: Box<Self>, expected_protocol_id: ProtocolId) -> ListenResult {
        let mut auth_senders = Vec::with_capacity(self.sockets.len());
        let mut auth_receivers = Vec::with_capacity(self.sockets.len());
        let mut packet_senders = Vec::with_capacity(self.sockets.len());
        let mut packet_receivers = Vec::with_capacity(self.sockets.len());
        for socket in self.sockets {
            let (auth_sender, auth_receiver, packet_sender, packet_receiver) =
                socket.listen(expected_protocol_id);
            auth_senders.push(auth_sender);
            auth_receivers.push(auth_receiver);
            packet_senders.push(packet_sender);
            packet_receivers.push(packet_receiver);
        }
        (
            Box::new(MultiAuthSender {
                senders: auth_senders,
            }),
            Box::new(MultiAuthReceiver {
                receivers: auth_receivers,
                last_payload: None,
            }),
            Box::new(MultiPacketSender {
                senders: packet_senders,
            }),
            Box::new(MultiPacketReceiver {
                receivers: packet_receivers,
                last_payload: None,
            }),
        )
    }
}

/// Fan-out sender: first transport that accepts the address wins.
#[derive(Clone)]
struct MultiPacketSender {
    senders: Vec<Box<dyn PacketSender>>,
}

impl PacketSender for MultiPacketSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        for sender in &self.senders {
            if sender.send(address, payload).is_ok() {
                return Ok(());
            }
        }
        Err(SendError)
    }
}

/// Fan-in receiver: polls inners in order, first packet wins.
///
/// Buffers the winning payload in `self` (like `LocalServerReceiver` does)
/// so the returned slice never borrows across the inner dispatch.
#[derive(Clone)]
struct MultiPacketReceiver {
    receivers: Vec<Box<dyn PacketReceiver>>,
    last_payload: Option<(SocketAddr, Box<[u8]>)>,
}

impl PacketReceiver for MultiPacketReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        for receiver in self.receivers.iter_mut() {
            if let Some((addr, bytes)) = receiver.receive()? {
                self.last_payload = Some((addr, bytes.to_vec().into_boxed_slice()));
                let (addr, payload) = self.last_payload.as_ref().unwrap();
                return Ok(Some((*addr, payload.as_ref())));
            }
        }
        self.last_payload = None;
        Ok(None)
    }

    fn readiness(&self) -> Option<PacketReadiness> {
        self.receivers.iter().find_map(|r| r.readiness())
    }
}

/// Fan-out auth sender: first transport that accepts the address wins.
struct MultiAuthSender {
    senders: Vec<Box<dyn AuthSender>>,
}

impl AuthSender for MultiAuthSender {
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        for sender in &self.senders {
            if sender.accept(address, identity_token).is_ok() {
                return Ok(());
            }
        }
        Err(SendError)
    }

    fn reject(&self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError> {
        for sender in &self.senders {
            if sender.reject(address, payload).is_ok() {
                return Ok(());
            }
        }
        Err(SendError)
    }
}

/// Fan-in auth receiver: polls inners in order, first payload wins.
/// Buffers like [`MultiPacketReceiver`]: the trait hands out borrows.
#[derive(Clone)]
struct MultiAuthReceiver {
    receivers: Vec<Box<dyn AuthReceiver>>,
    last_payload: Option<(SocketAddr, Box<[u8]>)>,
}

impl AuthReceiver for MultiAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        for receiver in self.receivers.iter_mut() {
            if let Some((addr, bytes)) = receiver.receive()? {
                self.last_payload = Some((addr, bytes.to_vec().into_boxed_slice()));
                let (addr, payload) = self.last_payload.as_ref().unwrap();
                return Ok(Some((*addr, payload.as_ref())));
            }
        }
        self.last_payload = None;
        Ok(None)
    }
}
