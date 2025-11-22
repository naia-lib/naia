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

const FAKE_CLIENT_ADDR: &str = "127.0.0.1:12345";
const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

/// Pair of connected server and client sockets for E2E testing
pub struct LocalSocketPair {
    pub server_sender: Box<dyn ServerPacketSender>,
    pub server_receiver: Box<dyn ServerPacketReceiver>,
    pub client_sender: Box<dyn ClientPacketSender>,
    pub client_receiver: Box<dyn ClientPacketReceiver>,
    pub client_identity: Box<dyn ClientIdentityReceiver>,
}

impl LocalSocketPair {
    pub fn new() -> Self {
        let client_addr: SocketAddr = FAKE_CLIENT_ADDR.parse().unwrap();
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().unwrap();
        
        // Create shared packet queues
        let server_to_client_queue = Arc::new(Mutex::new(VecDeque::new()));
        let client_to_server_queue = Arc::new(Mutex::new(VecDeque::new()));
        
        // Server socket
        let server_sender = Box::new(LocalServerSender {
            queue: server_to_client_queue.clone(),
            client_addr,
        });
        let server_receiver = Box::new(LocalServerReceiver {
            queue: client_to_server_queue.clone(),
            client_addr,
            last_payload: Arc::new(Mutex::new(None)),
        });
        
        // Client socket
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
            server_sender,
            server_receiver,
            client_sender,
            client_receiver,
            client_identity,
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
        IdentityReceiverResult::Waiting
    }
}

// Note: Clone implementations for boxed traits are automatically provided by the socket crates
// via blanket implementations for T: PacketSender + Clone

