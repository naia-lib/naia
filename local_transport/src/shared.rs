use std::{collections::VecDeque, net::SocketAddr, sync::{Arc, Mutex}};
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

pub struct ClientSendError;
pub struct ClientRecvError;
pub struct ServerSendError;
pub struct ServerRecvError;

// Error type for local auth operations
#[derive(Debug)]
pub(crate) enum LocalAuthError {
    ChannelClosed,
    ParseError,
}

// Shared queues for data packets (auth has dedicated 1:1 channels now)
#[derive(Clone)]
pub(crate) struct LocalTransportQueues {
    pub(crate) client_to_server: Arc<Mutex<VecDeque<Vec<u8>>>>,
    pub(crate) server_to_client: Arc<Mutex<VecDeque<Vec<u8>>>>,
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
                client_to_server: Arc::new(Mutex::new(VecDeque::new())),
                server_to_client: Arc::new(Mutex::new(VecDeque::new())),
                identity_token: Arc::new(Mutex::new(None)),
                rejection_code: Arc::new(Mutex::new(None)),
                server_data_addr: server_addr,
            },
            client_addr,
            server_addr,
        )
    }
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

