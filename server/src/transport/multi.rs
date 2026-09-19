//! Fan-in server transport: serve one naia `Server` from several transports
//! at once (naia-lib/naia#207 — e.g. UDP for standalone plus WebRTC for
//! Wasm behind a single `server.listen(multi)` call).
//!
//! Routing is by address ORIGIN, never by guess: every inbound packet or
//! auth request records which inner it arrived on, and sends go back through
//! that same inner. A send to an address no inner ever saw is an error —
//! real UDP/WebRTC sends to unseen addresses usually still return `Ok`, so
//! "first Ok wins" would silently put every reply on inner 0. Receives poll
//! the inners round-robin so a busy inner cannot starve the rest.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use naia_shared::{IdentityToken, ProtocolId};
use parking_lot::Mutex;

use super::{
    AuthReceiver, AuthSender, ListenResult, PacketReadiness, PacketReceiver, PacketSender,
    RecvError, SendError, Socket,
};

/// Address → index of the inner transport that last delivered it.
type OriginMap = Arc<Mutex<HashMap<SocketAddr, usize>>>;

/// Serves one [`Socket`] endpoint from several transports at once.
///
/// Client address spaces are disjoint across transports in practice (a UDP
/// peer address never equals a WebRTC session address); when two inners do
/// report the same address, the most recently seen origin wins.
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
        let origins: OriginMap = Arc::new(Mutex::new(HashMap::new()));
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
                origins: origins.clone(),
            }),
            Box::new(MultiAuthReceiver {
                receivers: auth_receivers,
                origins: origins.clone(),
                start: 0,
                last_payload: None,
            }),
            Box::new(MultiPacketSender {
                senders: packet_senders,
                origins: origins.clone(),
            }),
            Box::new(MultiPacketReceiver {
                receivers: packet_receivers,
                origins,
                start: 0,
                last_payload: None,
            }),
        )
    }
}

/// Origin-routed sender: the address's recorded inner, or an error.
#[derive(Clone)]
struct MultiPacketSender {
    senders: Vec<Box<dyn PacketSender>>,
    origins: OriginMap,
}

impl PacketSender for MultiPacketSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        let origins = self.origins.lock();
        match origins.get(address) {
            Some(&index) => self.senders[index].send(address, payload),
            None => Err(SendError),
        }
    }
}

/// Round-robin receiver: each poll starts one inner further along, so a busy
/// inner cannot starve the rest. Inbound addresses are recorded as origins.
///
/// Buffers the winning payload in `self` (like `LocalServerReceiver` does)
/// so the returned slice never borrows across the inner dispatch.
#[derive(Clone)]
struct MultiPacketReceiver {
    receivers: Vec<Box<dyn PacketReceiver>>,
    origins: OriginMap,
    start: usize,
    last_payload: Option<(SocketAddr, Box<[u8]>)>,
}

impl PacketReceiver for MultiPacketReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        let count = self.receivers.len();
        let start = self.start;
        self.start = (self.start + 1) % count;
        for step in 0..count {
            let index = (start + step) % count;
            if let Some((addr, bytes)) = self.receivers[index].receive()? {
                self.origins.lock().insert(addr, index);
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

/// Origin-routed auth sender: accept/reject go back through the inner the
/// client's auth request arrived on; unknown addresses are an error.
struct MultiAuthSender {
    senders: Vec<Box<dyn AuthSender>>,
    origins: OriginMap,
}

impl AuthSender for MultiAuthSender {
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        let origins = self.origins.lock();
        match origins.get(address) {
            Some(&index) => self.senders[index].accept(address, identity_token),
            None => Err(SendError),
        }
    }

    fn reject(&self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError> {
        let origins = self.origins.lock();
        match origins.get(address) {
            Some(&index) => self.senders[index].reject(address, payload),
            None => Err(SendError),
        }
    }
}

/// Round-robin auth receiver with origin recording, like [`MultiPacketReceiver`].
#[derive(Clone)]
struct MultiAuthReceiver {
    receivers: Vec<Box<dyn AuthReceiver>>,
    origins: OriginMap,
    start: usize,
    last_payload: Option<(SocketAddr, Box<[u8]>)>,
}

impl AuthReceiver for MultiAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        let count = self.receivers.len();
        let start = self.start;
        self.start = (self.start + 1) % count;
        for step in 0..count {
            let index = (start + step) % count;
            if let Some((addr, bytes)) = self.receivers[index].receive()? {
                self.origins.lock().insert(addr, index);
                self.last_payload = Some((addr, bytes.to_vec().into_boxed_slice()));
                let (addr, payload) = self.last_payload.as_ref().unwrap();
                return Ok(Some((*addr, payload.as_ref())));
            }
        }
        self.last_payload = None;
        Ok(None)
    }
}
