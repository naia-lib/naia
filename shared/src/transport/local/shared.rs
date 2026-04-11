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

// Helper to create data packet channels
pub(crate) fn create_data_channels() -> (
    mpsc::Sender<Vec<u8>>,   // client sends to server
    mpsc::Receiver<Vec<u8>>, // server receives from client
    mpsc::Sender<Vec<u8>>,   // server sends to client
    mpsc::Receiver<Vec<u8>>, // client receives from server
) {
    let (client_tx, server_rx) = mpsc::channel();
    let (server_tx, client_rx) = mpsc::channel();
    (client_tx, server_rx, server_tx, client_rx)
}

// Helper to create 1:1 auth channels
pub(crate) fn create_auth_channels() -> (
    mpsc::Sender<Vec<u8>>,   // client sends requests
    mpsc::Receiver<Vec<u8>>, // server receives requests
    mpsc::Sender<Vec<u8>>,   // server sends responses
    mpsc::Receiver<Vec<u8>>, // client receives responses
) {
    let (req_tx, req_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();
    (req_tx, req_rx, resp_tx, resp_rx)
}
