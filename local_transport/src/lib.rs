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
    auth_requests: Arc<Mutex<VecDeque<Vec<u8>>>>,  // Now stores HTTP request bytes
    auth_responses: Arc<Mutex<VecDeque<Vec<u8>>>>, // Stores HTTP response bytes
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
    addr_cell: LocalAddrCell,
    server_data_addr: SocketAddr, // For including in HTTP response
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
                auth_responses: Arc::new(Mutex::new(VecDeque::new())),
                identity_token: Arc::new(Mutex::new(None)),
                rejection_code: Arc::new(Mutex::new(None)),
                addr_cell: LocalAddrCell::new(),
                server_data_addr: server_addr,
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
    auth_responses: Arc<Mutex<VecDeque<Vec<u8>>>>,
    server_data_addr: SocketAddr,
}

impl LocalServerAuthSender {
    fn new(shared: LocalTransportQueues) -> Self {
        Self {
            identity_token: shared.identity_token.clone(),
            rejection_code: shared.rejection_code.clone(),
            auth_responses: shared.auth_responses.clone(),
            server_data_addr: shared.server_data_addr,
        }
    }

    pub fn accept(&self, _address: &SocketAddr, identity_token: &IdentityToken) -> Result<(), ServerSendError> {
        // Build HTTP 200 response with identity token and server address in body
        let response_body = format!("{}\r\n{}", identity_token, self.server_data_addr);
        let response = http::Response::builder()
            .status(200)
            .body(response_body.into_bytes())
            .unwrap();
        
        let response_bytes = naia_shared::http_utils::response_to_bytes(response);
        
        // Store in auth_responses queue
        if let Ok(mut queue) = self.auth_responses.lock() {
            queue.push_back(response_bytes);
            log::debug!("[LocalTransport] Server sent HTTP 200 response with identity token");
        }
        
        *self.identity_token.lock().unwrap() = Some(identity_token.clone());
        *self.rejection_code.lock().unwrap() = None;
        Ok(())
    }

    pub fn reject(&self, _address: &SocketAddr) -> Result<(), ServerSendError> {
        // Build HTTP 401 response
        let response = http::Response::builder()
            .status(401)
            .body(Vec::new())
            .unwrap();
        
        let response_bytes = naia_shared::http_utils::response_to_bytes(response);
        
        // Store in auth_responses queue
        if let Ok(mut queue) = self.auth_responses.lock() {
            queue.push_back(response_bytes);
            log::debug!("[LocalTransport] Server sent HTTP 401 rejection response");
        }
        
        *self.rejection_code.lock().unwrap() = Some(401);
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
        if let Some(request_bytes) = queue.pop_front() {
            log::trace!("[LocalTransport] Server received HTTP auth request");
            
            // Parse HTTP request
            let request = naia_shared::http_utils::bytes_to_request(&request_bytes);
            
            // Extract Authorization header if present
            if let Some(auth_header) = request.headers().get("Authorization") {
                let auth_str = auth_header.to_str().unwrap();
                let auth_bytes = base64::decode(auth_str).unwrap();
                let boxed = auth_bytes.into_boxed_slice();
                *self.last_payload.lock().unwrap() = Some(boxed);
                let payload_ref = self.last_payload.lock().unwrap();
                let payload_slice = payload_ref.as_ref().unwrap().as_ref();
                let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice) };
                Ok(Some((self.client_addr, static_ref)))
            } else {
                // No auth header present, return empty auth (for connect_with_auth_headers case)
                Ok(None)
            }
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
        // Build HTTP POST request with Authorization header
        let base64_encoded = base64::encode(&auth_bytes);
        let request = http::Request::builder()
            .method("POST")
            .uri("/")
            .header("Authorization", base64_encoded)
            .body(Vec::new())
            .unwrap();
        
        let request_bytes = naia_shared::http_utils::request_to_bytes(request);
        
        if let Ok(mut queue) = self.auth_queue.lock() {
            queue.push_back(request_bytes);
            log::trace!("[LocalTransport] Client sent HTTP auth request to server");
        }
        self.connect()
    }

    pub fn connect_with_auth_headers(
        self,
        auth_headers: Vec<(String, String)>,
    ) -> (
        LocalClientIdentity,
        LocalClientSender,
        LocalClientReceiver,
    ) {
        // Build HTTP POST request with custom headers
        let mut builder = http::Request::builder()
            .method("POST")
            .uri("/");
        
        for (key, value) in auth_headers {
            builder = builder.header(key, value);
        }
        
        let request = builder.body(Vec::new()).unwrap();
        let request_bytes = naia_shared::http_utils::request_to_bytes(request);
        
        if let Ok(mut queue) = self.auth_queue.lock() {
            queue.push_back(request_bytes);
            log::trace!("[LocalTransport] Client sent HTTP auth request with headers to server");
        }
        self.connect()
    }

    pub fn connect_with_auth_and_headers(
        self,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (
        LocalClientIdentity,
        LocalClientSender,
        LocalClientReceiver,
    ) {
        // Build HTTP POST request with both auth and headers
        let base64_encoded = base64::encode(&auth_bytes);
        let mut builder = http::Request::builder()
            .method("POST")
            .uri("/")
            .header("Authorization", base64_encoded);
        
        for (key, value) in auth_headers {
            builder = builder.header(key, value);
        }
        
        let request = builder.body(Vec::new()).unwrap();
        let request_bytes = naia_shared::http_utils::request_to_bytes(request);
        
        if let Ok(mut queue) = self.auth_queue.lock() {
            queue.push_back(request_bytes);
            log::trace!("[LocalTransport] Client sent HTTP auth request with auth and headers to server");
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
    auth_responses: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl LocalClientIdentity {
    fn new(shared: LocalTransportQueues, _client_addr: SocketAddr, _server_addr: SocketAddr) -> Self {
        let addr_cell = shared.addr_cell.clone();
        // Don't initialize server address here - it will be set via HTTP response parsing
        Self {
            identity_token: shared.identity_token.clone(),
            rejection_code: shared.rejection_code.clone(),
            requested: Arc::new(Mutex::new(false)),
            addr_cell,
            auth_responses: shared.auth_responses.clone(),
        }
    }

    pub fn receive(&mut self) -> ClientIdentityReceiverResult {
        // Check auth_responses queue for HTTP response
        if let Ok(mut queue) = self.auth_responses.lock() {
            if let Some(response_bytes) = queue.pop_front() {
                log::trace!("[LocalTransport] Client received HTTP auth response");
                let response = naia_shared::http_utils::bytes_to_response(&response_bytes);
                let status_code = response.status().as_u16();
                
                if status_code != 200 {
                    *self.rejection_code.lock().unwrap() = Some(status_code);
                    log::trace!("[LocalTransport] Client identity receiver: ErrorResponseCode({})", status_code);
                    return ClientIdentityReceiverResult::ErrorResponseCode(status_code);
                }
                
                // Parse response body: "identity_token\r\nserver_addr"
                let body = String::from_utf8_lossy(response.body());
                let mut parts = body.splitn(2, "\r\n");
                let identity_token = parts.next().unwrap().to_string();
                let server_addr_str = parts.next().unwrap();
                let server_addr: SocketAddr = server_addr_str.parse().unwrap();
                
                // Update addr_cell with server address
                self.addr_cell.recv(server_addr);
                log::trace!("[LocalTransport] Client discovered server address: {}", server_addr);
                
                *self.identity_token.lock().unwrap() = Some(identity_token.clone());
                log::trace!("[LocalTransport] Client identity receiver: Success(token={})", identity_token);
                return ClientIdentityReceiverResult::Success(identity_token);
            }
        }
        
        // Check if rejection happened
        if let Some(code) = *self.rejection_code.lock().unwrap() {
            log::trace!("[LocalTransport] Client identity receiver: ErrorResponseCode({})", code);
            return ClientIdentityReceiverResult::ErrorResponseCode(code);
        }
        
        // Check if already received token (from previous call)
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
