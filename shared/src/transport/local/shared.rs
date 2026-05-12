use std::net::SocketAddr;
use std::sync::mpsc;

use naia_socket_shared::IdentityToken;

pub const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

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
#[derive(Debug, Clone)]
pub enum LocalAuthError {
    ChannelClosed,
    ParseError,
}

/// (client→server sender, server receives, server→client sender, client receives)
pub(crate) type DataChannels = (
    mpsc::Sender<Vec<u8>>,
    mpsc::Receiver<Vec<u8>>,
    mpsc::Sender<Vec<u8>>,
    mpsc::Receiver<Vec<u8>>,
);

/// (client→server sender, server receives, server→client sender, client receives)
pub(crate) type AuthChannels = (
    mpsc::Sender<Vec<u8>>,
    mpsc::Receiver<Vec<u8>>,
    mpsc::Sender<Vec<u8>>,
    mpsc::Receiver<Vec<u8>>,
);

// Helper to create data packet channels
pub(crate) fn create_data_channels() -> DataChannels {
    let (client_tx, server_rx) = mpsc::channel();
    let (server_tx, client_rx) = mpsc::channel();
    (client_tx, server_rx, server_tx, client_rx)
}

// Helper to create 1:1 auth channels
pub(crate) fn create_auth_channels() -> AuthChannels {
    let (req_tx, req_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();
    (req_tx, req_rx, resp_tx, resp_rx)
}
