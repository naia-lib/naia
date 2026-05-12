use std::net::SocketAddr;
use std::sync::mpsc;

use naia_socket_shared::IdentityToken;

#[doc(hidden)]
pub const FAKE_SERVER_ADDR: &str = "127.0.0.1:54321";

// Types matching transport layer signatures
#[doc(hidden)]
pub enum ClientIdentityReceiverResult {
    /// Waiting for a response.
    Waiting,
    /// Successfully received an identity token.
    Success(IdentityToken),
    /// Received an HTTP error response code.
    ErrorResponseCode(u16),
}

#[doc(hidden)]
#[derive(Clone)]
pub enum ClientServerAddr {
    /// Server address has been resolved.
    Found(SocketAddr),
    /// Server address resolution is still in progress.
    Finding,
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ClientSendError;
#[doc(hidden)]
#[derive(Debug)]
pub struct ClientRecvError;
#[doc(hidden)]
#[derive(Debug)]
pub struct ServerSendError;
#[doc(hidden)]
#[derive(Debug)]
pub struct ServerRecvError;

// Error type for local auth operations
#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum LocalAuthError {
    /// The channel was closed unexpectedly.
    ChannelClosed,
    /// Failed to parse the auth payload.
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
