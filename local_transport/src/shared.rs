use std::{net::SocketAddr, sync::{Arc, Mutex}};
use naia_shared::IdentityToken;
use tokio::sync::mpsc;

pub(crate) const FAKE_CLIENT_ADDR: &str = "127.0.0.1:12345";
pub(crate) const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

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

#[derive(Debug)]
pub struct ClientSendError;
#[derive(Debug)]
pub struct ClientRecvError;
#[derive(Debug)]
pub struct ServerSendError;
#[derive(Debug)]
pub struct ServerRecvError;

// Error type for local auth operations
#[derive(Debug)]
pub(crate) enum LocalAuthError {
    ChannelClosed,
    ParseError,
}

// Shared state for transport (data packets now use channels too)
#[derive(Clone)]
pub(crate) struct LocalTransportQueues {
    pub(crate) identity_token: Arc<Mutex<Option<IdentityToken>>>,
    pub(crate) rejection_code: Arc<Mutex<Option<u16>>>,
    pub(crate) server_data_addr: SocketAddr, // For including in HTTP response
}

impl LocalTransportQueues {
    pub(crate) fn new() -> (Self, SocketAddr, SocketAddr) {
        let client_addr: SocketAddr = FAKE_CLIENT_ADDR.parse().expect("invalid client addr");
        let server_addr: SocketAddr = FAKE_SERVER_ADDR.parse().expect("invalid server addr");
        
        (
            Self {
                identity_token: Arc::new(Mutex::new(None)),
                rejection_code: Arc::new(Mutex::new(None)),
                server_data_addr: server_addr,
            },
            client_addr,
            server_addr,
        )
    }
}

// Helper to create data packet channels
pub(crate) fn create_data_channels() -> (
    mpsc::UnboundedSender<Vec<u8>>,  // client sends to server
    mpsc::UnboundedReceiver<Vec<u8>>, // server receives from client
    mpsc::UnboundedSender<Vec<u8>>,  // server sends to client
    mpsc::UnboundedReceiver<Vec<u8>>, // client receives from server
) {
    let (client_tx, server_rx) = mpsc::unbounded_channel();
    let (server_tx, client_rx) = mpsc::unbounded_channel();
    (client_tx, server_rx, server_tx, client_rx)
}

// Helper to create 1:1 auth channels
pub(crate) fn create_auth_channels() -> (
    mpsc::UnboundedSender<Vec<u8>>,  // client sends requests
    mpsc::UnboundedReceiver<Vec<u8>>, // server receives requests
    mpsc::UnboundedSender<Vec<u8>>,  // server sends responses
    mpsc::UnboundedReceiver<Vec<u8>>, // client receives responses
) {
    let (req_tx, req_rx) = mpsc::unbounded_channel();
    let (resp_tx, resp_rx) = mpsc::unbounded_channel();
    (req_tx, req_rx, resp_tx, resp_rx)
}

