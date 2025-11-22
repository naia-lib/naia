//! In-memory local transport layers used by `transport_local` feature flag for quick E2E tests.
//!
//! Provides paired client/server sockets with types that match the transport-level trait signatures.
//! The transport layer modules wrap these types to implement the transport traits.

use std::{collections::VecDeque, net::SocketAddr, sync::{Arc, Mutex}};

use naia_shared::IdentityToken;

const FAKE_CLIENT_ADDR: &str = "127.0.0.1:12345";
const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

// AddrCell equivalent for server address discovery
// Uses Mutex for now (synchronous), can be upgraded to RwLock for full async support
struct MaybeAddr(ClientServerAddr);

#[derive(Clone)]
pub struct LocalAddrCell {
    cell: Arc<Mutex<MaybeAddr>>,
}

impl LocalAddrCell {
    pub fn new() -> Self {
        Self {
            cell: Arc::new(Mutex::new(MaybeAddr(ClientServerAddr::Finding))),
        }
    }

    pub fn recv(&self, addr: SocketAddr) {
        let mut cell = self.cell.lock().unwrap();
        cell.0 = ClientServerAddr::Found(addr);
    }

    pub async fn recv_async(&self, addr: SocketAddr) {
        // For future async use (Phase 4)
        self.recv(addr);
    }

    pub fn get(&self) -> ClientServerAddr {
        match self.cell.try_lock() {
            Ok(addr) => addr.0.clone(),
            Err(_) => ClientServerAddr::Finding,
        }
    }
}

// Types matching transport layer signatures
pub enum ClientIdentityReceiverResult {
    Waiting,
    Success(IdentityToken),
    ErrorResponseCode(u16),
}

#[derive(Clone)]
pub enum ClientServerAddr {
    Found(SocketAddr),
    Finding,
}

pub struct ClientSendError;
pub struct ClientRecvError;
pub struct ServerSendError;
pub struct ServerRecvError;

#[derive(Clone)]
struct LocalTransportQueues {
    client_to_server: Arc<Mutex<VecDeque<Vec<u8>>>>,
    server_to_client: Arc<Mutex<VecDeque<Vec<u8>>>>,
    auth_requests: Arc<Mutex<VecDeque<Vec<u8>>>>,
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
    addr_cell: LocalAddrCell,
}

impl LocalTransportQueues {
    fn new() -> (Self, SocketAddr, SocketAddr) {
        let client_addr: SocketAddr = FAKE_CLIENT_ADDR.parse().expect("invalid client addr");
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid server addr");
        (
            Self {
                client_to_server: Arc::new(Mutex::new(VecDeque::new())),
                server_to_client: Arc::new(Mutex::new(VecDeque::new())),
                auth_requests: Arc::new(Mutex::new(VecDeque::new())),
                identity_token: Arc::new(Mutex::new(None)),
                rejection_code: Arc::new(Mutex::new(None)),
                addr_cell: LocalAddrCell::new(),
            },
            client_addr,
            server_addr,
        )
    }
}

/// Paired sockets for the client and server sides.
pub struct LocalSocketPair {
    pub client_socket: LocalClientSocket,
    pub server_socket: LocalServerSocket,
}

impl LocalSocketPair {
    pub fn new() -> Self {
        let (shared, client_addr, server_addr) = LocalTransportQueues::new();
        let client = LocalClientSocket::new(shared.clone(), client_addr, server_addr);
        let server = LocalServerSocket::new(shared, client_addr, server_addr);
        Self {
            client_socket: client,
            server_socket: server,
        }
    }
}

// ============================================================================
// Server-side implementation
// ============================================================================

pub struct LocalServerSocket {
    sender: LocalServerSender,
    receiver: LocalServerReceiver,
    auth_sender: LocalServerAuthSender,
    auth_receiver: LocalServerAuthReceiver,
}

impl LocalServerSocket {
    fn new(shared: LocalTransportQueues, client_addr: SocketAddr, _server_addr: SocketAddr) -> Self {
        let auth_sender = LocalServerAuthSender::new(shared.clone());
        let auth_receiver = LocalServerAuthReceiver::new(shared.clone(), client_addr);
        Self {
            sender: LocalServerSender::new(shared.server_to_client.clone(), client_addr),
            receiver: LocalServerReceiver::new(shared.client_to_server.clone(), client_addr),
            auth_sender,
            auth_receiver,
        }
    }

    pub fn listen_with_auth(
        self,
    ) -> (
        LocalServerAuthSender,
        LocalServerAuthReceiver,
        LocalServerSender,
        LocalServerReceiver,
    ) {
        let LocalServerSocket {
            sender,
            receiver,
            auth_sender,
            auth_receiver,
        } = self;
        (auth_sender, auth_receiver, sender, receiver)
    }
}

// Packet send/receive
#[derive(Clone)]
pub struct LocalServerSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
}

impl LocalServerSender {
    fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, client_addr: SocketAddr) -> Self {
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

#[derive(Clone)]
pub struct LocalServerReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalServerReceiver {
    fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, client_addr: SocketAddr) -> Self {
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

// Auth send/receive
#[derive(Clone)]
pub struct LocalServerAuthSender {
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
}

impl LocalServerAuthSender {
    fn new(shared: LocalTransportQueues) -> Self {
        Self {
            identity_token: shared.identity_token.clone(),
            rejection_code: shared.rejection_code.clone(),
        }
    }

    pub fn accept(&self, _address: &SocketAddr, identity_token: &IdentityToken) -> Result<(), ServerSendError> {
        *self.identity_token.lock().unwrap() = Some(identity_token.clone());
        *self.rejection_code.lock().unwrap() = None;
        log::debug!("[LocalTransport] Server accepted identity");
        Ok(())
    }

    pub fn reject(&self, _address: &SocketAddr) -> Result<(), ServerSendError> {
        *self.rejection_code.lock().unwrap() = Some(401);
        log::debug!("[LocalTransport] Server rejected connection");
        Ok(())
    }
}

#[derive(Clone)]
pub struct LocalServerAuthReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalServerAuthReceiver {
    fn new(shared: LocalTransportQueues, client_addr: SocketAddr) -> Self {
        Self {
            queue: shared.auth_requests.clone(),
            client_addr,
            last_payload: Arc::new(Mutex::new(None)),
        }
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(payload) = queue.pop_front() {
            log::trace!("[LocalTransport] Server received auth request");
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

// ============================================================================
// Client-side implementation
// ============================================================================

pub struct LocalClientSocket {
    identity: LocalClientIdentity,
    sender: LocalClientSender,
    receiver: LocalClientReceiver,
    auth_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl LocalClientSocket {
    fn new(shared: LocalTransportQueues, client_addr: SocketAddr, server_addr: SocketAddr) -> Self {
        let identity = LocalClientIdentity::new(shared.clone(), client_addr, server_addr);
        Self {
            identity,
            sender: LocalClientSender::new(shared.client_to_server.clone(), shared.addr_cell.clone()),
            receiver: LocalClientReceiver::new(shared.server_to_client.clone(), shared.addr_cell.clone()),
            auth_queue: shared.auth_requests.clone(),
        }
    }

    pub fn connect(
        self,
    ) -> (
        LocalClientIdentity,
        LocalClientSender,
        LocalClientReceiver,
    ) {
        let LocalClientSocket { identity, sender, receiver, .. } = self;
        (identity, sender, receiver)
    }

    pub fn connect_with_auth(
        self,
        auth_bytes: Vec<u8>,
    ) -> (
        LocalClientIdentity,
        LocalClientSender,
        LocalClientReceiver,
    ) {
        // Send auth bytes to server's auth receiver
        if let Ok(mut queue) = self.auth_queue.lock() {
            queue.push_back(auth_bytes);
            log::trace!("[LocalTransport] Client sent auth bytes to server");
        }
        self.connect()
    }
}

#[derive(Clone)]
pub struct LocalClientSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
}

impl LocalClientSender {
    fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, addr_cell: LocalAddrCell) -> Self {
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

#[derive(Clone)]
pub struct LocalClientReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    addr_cell: LocalAddrCell,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl LocalClientReceiver {
    fn new(queue: Arc<Mutex<VecDeque<Vec<u8>>>>, addr_cell: LocalAddrCell) -> Self {
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

#[derive(Clone)]
pub struct LocalClientIdentity {
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
    requested: Arc<Mutex<bool>>,
    addr_cell: LocalAddrCell,
}

impl LocalClientIdentity {
    fn new(shared: LocalTransportQueues, _client_addr: SocketAddr, server_addr: SocketAddr) -> Self {
        let addr_cell = shared.addr_cell.clone();
        // Initialize server address synchronously for backward compatibility
        // In Phase 4, this will be set via HTTP response parsing
        addr_cell.recv(server_addr);
        Self {
            identity_token: shared.identity_token.clone(),
            rejection_code: shared.rejection_code.clone(),
            requested: Arc::new(Mutex::new(false)),
            addr_cell,
        }
    }

    pub fn receive(&mut self) -> ClientIdentityReceiverResult {
        if let Some(code) = *self.rejection_code.lock().unwrap() {
            log::trace!("[LocalTransport] Client identity receiver: ErrorResponseCode({})", code);
            return ClientIdentityReceiverResult::ErrorResponseCode(code);
        }
        if let Some(token) = self.identity_token.lock().unwrap().clone() {
            log::trace!("[LocalTransport] Client identity receiver: Success(token={})", token);
            return ClientIdentityReceiverResult::Success(token);
        }

        let mut requested = self.requested.lock().unwrap();
        if !*requested {
            *requested = true;
            log::trace!("[LocalTransport] Client requesting identity");
        } else {
            log::trace!("[LocalTransport] Client identity receiver: Still waiting...");
        }

        ClientIdentityReceiverResult::Waiting
    }
}
