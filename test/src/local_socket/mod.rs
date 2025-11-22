/// In-memory socket implementation for E2E testing
/// Routes packets between server and client without network I/O

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use naia_server_socket::{
    PacketReceiver as ServerPacketReceiver,
    PacketSender as ServerPacketSender,
    NaiaServerSocketError,
};
use naia_client_socket::{
    PacketReceiver as ClientPacketReceiver,
    PacketSender as ClientPacketSender,
    IdentityReceiver as ClientIdentityReceiver,
    ServerAddr,
    NaiaClientSocketError,
};

// Import transport Socket traits
use naia_server::transport::{
    Socket as ServerSocket,
    AuthSender as ServerAuthSender,
    AuthReceiver as ServerAuthReceiver,
    PacketSender as ServerTransportSender,
    PacketReceiver as ServerTransportReceiver,
    SendError as ServerTransportSendError,
    RecvError as ServerTransportRecvError,
};
use naia_client::transport::{
    Socket as ClientSocket,
    IdentityReceiver as ClientTransportIdentityReceiver,
    IdentityReceiverResult as ClientTransportIdentityReceiverResult,
    PacketSender as ClientTransportSender,
    PacketReceiver as ClientTransportReceiver,
    SendError as ClientTransportSendError,
    RecvError as ClientTransportRecvError,
    ServerAddr as ClientTransportServerAddr,
};

const FAKE_CLIENT_ADDR: &str = "127.0.0.1:12345";
const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

/// Pair of connected server and client sockets for E2E testing
pub struct LocalSocketPair {
    pub server_socket: LocalServerSocket,
    pub client_socket: LocalClientSocket,
}

impl LocalSocketPair {
    pub fn new() -> Self {
        let client_addr: SocketAddr = FAKE_CLIENT_ADDR.parse().unwrap();
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().unwrap();
        
        // Create shared packet queues
        let server_to_client_queue = Arc::new(Mutex::new(VecDeque::new()));
        let client_to_server_queue = Arc::new(Mutex::new(VecDeque::new()));
        
        // Server socket components
        let server_sender = Box::new(LocalServerSender {
            queue: server_to_client_queue.clone(),
            client_addr,
        });
        let server_receiver = Box::new(LocalServerReceiver {
            queue: client_to_server_queue.clone(),
            client_addr,
            last_payload: Arc::new(Mutex::new(None)),
        });
        
        // Client socket components
        let client_sender = Box::new(LocalClientSender {
            queue: client_to_server_queue,
            server_addr,
            connected: Arc::new(Mutex::new(true)),
        });
        let client_receiver = Box::new(LocalClientReceiver {
            queue: server_to_client_queue,
            server_addr,
            last_payload: Arc::new(Mutex::new(None)),
        });
        let client_identity = Box::new(LocalClientIdentity {
            server_addr,
        });
        
        Self {
            server_socket: LocalServerSocket::new(server_sender, server_receiver),
            client_socket: LocalClientSocket::new(client_identity, client_sender, client_receiver),
        }
    }
}

// Server Socket Components

#[derive(Clone)]
struct LocalServerSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
}

impl ServerPacketSender for LocalServerSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), NaiaServerSocketError> {
        if address != &self.client_addr {
            return Err(NaiaServerSocketError::SendError(*address));
        }
        
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(payload.to_vec());
        Ok(())
    }
}

#[derive(Clone)]
struct LocalServerReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    client_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl ServerPacketReceiver for LocalServerReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, NaiaServerSocketError> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(packet) = queue.pop_front() {
            let boxed = packet.into_boxed_slice();
            *self.last_payload.lock().unwrap() = Some(boxed);
            let payload_ref = self.last_payload.lock().unwrap();
            let payload_slice = payload_ref.as_ref().unwrap().as_ref();
            // This is safe because we're holding the lock and the payload lives in Arc
            let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice) };
            Ok(Some((self.client_addr, static_ref)))
        } else {
            Ok(None)
        }
    }
}

// Client Socket Components

#[derive(Clone)]
struct LocalClientSender {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    server_addr: SocketAddr,
    connected: Arc<Mutex<bool>>,
}

impl ClientPacketSender for LocalClientSender {
    fn send(&self, payload: &[u8]) -> Result<(), NaiaClientSocketError> {
        let connected = *self.connected.lock().unwrap();
        if !connected {
            return Err(NaiaClientSocketError::SendError);
        }
        
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(payload.to_vec());
        Ok(())
    }
    
    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found(self.server_addr)
    }
    
    fn connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
    
    fn disconnect(&mut self) {
        *self.connected.lock().unwrap() = false;
    }
}

#[derive(Clone)]
struct LocalClientReceiver {
    queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    server_addr: SocketAddr,
    last_payload: Arc<Mutex<Option<Box<[u8]>>>>,
}

impl ClientPacketReceiver for LocalClientReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        let mut queue = self.queue.lock().unwrap();
        if let Some(packet) = queue.pop_front() {
            let boxed = packet.into_boxed_slice();
            *self.last_payload.lock().unwrap() = Some(boxed);
            let payload_ref = self.last_payload.lock().unwrap();
            let payload_slice = payload_ref.as_ref().unwrap().as_ref();
            // This is safe because we're holding the lock and the payload lives in Arc
            let static_ref: &'static [u8] = unsafe { std::mem::transmute(payload_slice) };
            Ok(Some(static_ref))
        } else {
            Ok(None)
        }
    }
    
    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found(self.server_addr)
    }
}

#[derive(Clone)]
struct LocalClientIdentity {
    server_addr: SocketAddr,
}

impl ClientIdentityReceiver for LocalClientIdentity {
    fn receive(&mut self) -> naia_client_socket::IdentityReceiverResult {
        // For testing, we immediately "receive" the server's address
        // Only return once to simulate initial connection
        use naia_client_socket::IdentityReceiverResult;
        IdentityReceiverResult::Success("test_token".to_string())
    }
}

// Note: Clone implementations for boxed traits are automatically provided by the socket crates
// via blanket implementations for T: PacketSender + Clone

// ============================================================================
// Socket Wrappers for Client and Server
// ============================================================================

/// Server-side Socket implementation for testing
pub struct LocalServerSocket {
    sender: Box<dyn ServerPacketSender>,
    receiver: Box<dyn ServerPacketReceiver>,
}

impl LocalServerSocket {
    pub fn new(sender: Box<dyn ServerPacketSender>, receiver: Box<dyn ServerPacketReceiver>) -> Self {
        Self { sender, receiver }
    }
}

/// Client-side Socket implementation for testing  
pub struct LocalClientSocket {
    identity: Box<dyn ClientIdentityReceiver>,
    sender: Box<dyn ClientPacketSender>,
    receiver: Box<dyn ClientPacketReceiver>,
}

impl LocalClientSocket {
    pub fn new(
        identity: Box<dyn ClientIdentityReceiver>,
        sender: Box<dyn ClientPacketSender>, 
        receiver: Box<dyn ClientPacketReceiver>,
    ) -> Self {
        Self { identity, sender, receiver }
    }
}

// ============================================================================
// Dummy Auth implementations for Server (not used in testing)
// ============================================================================

#[derive(Clone)]
struct DummyAuthSender;

impl ServerAuthSender for DummyAuthSender {
    fn accept(&self, _address: &SocketAddr, _identity_token: &String) -> Result<(), ServerTransportSendError> {
        Ok(())
    }
    fn reject(&self, _address: &SocketAddr) -> Result<(), ServerTransportSendError> {
        Ok(())
    }
}

#[derive(Clone)]
struct DummyAuthReceiver;

impl ServerAuthReceiver for DummyAuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerTransportRecvError> {
        Ok(None)
    }
}

// ============================================================================
// Transport Trait Implementations - Wrapper Approach
// ============================================================================

// Since we can't implement external traits for external types (orphan rule),
// we need to create wrapper types that we own

// Server-side wrappers
#[derive(Clone)]
pub struct LocalServerTransportSender(pub Box<dyn ServerPacketSender>);
#[derive(Clone)]
pub struct LocalServerTransportReceiver(pub Box<dyn ServerPacketReceiver>);

impl ServerTransportSender for LocalServerTransportSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), ServerTransportSendError> {
        self.0.send(address, payload).map_err(|_| ServerTransportSendError)
    }
}

impl ServerTransportReceiver for LocalServerTransportReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerTransportRecvError> {
        self.0.receive().map_err(|_| ServerTransportRecvError)
    }
}

// Client-side wrappers
#[derive(Clone)]
pub struct LocalClientTransportSender(pub Box<dyn ClientPacketSender>);
#[derive(Clone)]
pub struct LocalClientTransportReceiver(pub Box<dyn ClientPacketReceiver>);
#[derive(Clone)]
pub struct LocalClientTransportIdentityReceiver(pub Arc<Mutex<Box<dyn ClientIdentityReceiver>>>);

impl ClientTransportSender for LocalClientTransportSender {
    fn send(&self, payload: &[u8]) -> Result<(), ClientTransportSendError> {
        self.0.send(payload).map_err(|_| ClientTransportSendError)
    }
    
    fn server_addr(&self) -> ClientTransportServerAddr {
        match self.0.server_addr() {
            ServerAddr::Found(addr) => ClientTransportServerAddr::Found(addr),
            ServerAddr::Finding => ClientTransportServerAddr::Finding,
        }
    }
}

impl ClientTransportReceiver for LocalClientTransportReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, ClientTransportRecvError> {
        self.0.receive().map_err(|_| ClientTransportRecvError)
    }
    
    fn server_addr(&self) -> ClientTransportServerAddr {
        match self.0.server_addr() {
            ServerAddr::Found(addr) => ClientTransportServerAddr::Found(addr),
            ServerAddr::Finding => ClientTransportServerAddr::Finding,
        }
    }
}

impl ClientTransportIdentityReceiver for LocalClientTransportIdentityReceiver {
    fn receive(&mut self) -> ClientTransportIdentityReceiverResult {
        use naia_client_socket::IdentityReceiverResult;
        let mut receiver = self.0.lock().unwrap();
        match receiver.receive() {
            IdentityReceiverResult::Waiting => ClientTransportIdentityReceiverResult::Waiting,
            IdentityReceiverResult::Success(token) => ClientTransportIdentityReceiverResult::Success(token),
            IdentityReceiverResult::ErrorResponseCode(code) => ClientTransportIdentityReceiverResult::ErrorResponseCode(code),
        }
    }
}

// ============================================================================
// Trait Implementations for Server Socket
// ============================================================================

impl Into<Box<dyn ServerSocket>> for LocalServerSocket {
    fn into(self) -> Box<dyn ServerSocket> {
        Box::new(self)
    }
}

impl ServerSocket for LocalServerSocket {
    fn listen(
        self: Box<Self>,
    ) -> (
        Box<dyn ServerAuthSender>,
        Box<dyn ServerAuthReceiver>,
        Box<dyn ServerTransportSender>,
        Box<dyn ServerTransportReceiver>,
    ) {
        (
            Box::new(DummyAuthSender),
            Box::new(DummyAuthReceiver),
            Box::new(LocalServerTransportSender(self.sender)),
            Box::new(LocalServerTransportReceiver(self.receiver)),
        )
    }
}

// ============================================================================
// Trait Implementations for Client Socket
// ============================================================================

impl Into<Box<dyn ClientSocket>> for LocalClientSocket {
    fn into(self) -> Box<dyn ClientSocket> {
        Box::new(self)
    }
}

impl ClientSocket for LocalClientSocket {
    fn connect(
        self: Box<Self>,
    ) -> (
        Box<dyn ClientTransportIdentityReceiver>,
        Box<dyn ClientTransportSender>,
        Box<dyn ClientTransportReceiver>,
    ) {
        self.connect_inner()
    }

    fn connect_with_auth(
        self: Box<Self>,
        _auth_bytes: Vec<u8>,
    ) -> (
        Box<dyn ClientTransportIdentityReceiver>,
        Box<dyn ClientTransportSender>,
        Box<dyn ClientTransportReceiver>,
    ) {
        self.connect_inner()
    }

    fn connect_with_auth_headers(
        self: Box<Self>,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn ClientTransportIdentityReceiver>,
        Box<dyn ClientTransportSender>,
        Box<dyn ClientTransportReceiver>,
    ) {
        self.connect_inner()
    }

    fn connect_with_auth_and_headers(
        self: Box<Self>,
        _auth_bytes: Vec<u8>,
        _auth_headers: Vec<(String, String)>,
    ) -> (
        Box<dyn ClientTransportIdentityReceiver>,
        Box<dyn ClientTransportSender>,
        Box<dyn ClientTransportReceiver>,
    ) {
        self.connect_inner()
    }
}

impl LocalClientSocket {
    fn connect_inner(
        self: Box<Self>,
    ) -> (
        Box<dyn ClientTransportIdentityReceiver>,
        Box<dyn ClientTransportSender>,
        Box<dyn ClientTransportReceiver>,
    ) {
        // For testing, we ignore auth and just return the socket components wrapped
        (
            Box::new(LocalClientTransportIdentityReceiver(Arc::new(Mutex::new(self.identity)))),
            Box::new(LocalClientTransportSender(self.sender)),
            Box::new(LocalClientTransportReceiver(self.receiver)),
        )
    }
}

