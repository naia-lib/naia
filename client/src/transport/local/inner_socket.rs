use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use naia_shared::IdentityToken;
use std::sync::mpsc;

// use crate::shared::LocalTransportQueues;
use super::{
    addr_cell::LocalAddrCell,
    auth::{ClientAuthIo, LocalClientIdentity},
    data::{LocalClientReceiver, LocalClientSender},
};

pub struct LocalClientSocket {
    auth_io: Arc<Mutex<ClientAuthIo>>,
    sender: LocalClientSender,
    receiver: LocalClientReceiver,
    auth_requests_tx: mpsc::Sender<Vec<u8>>,
}

impl LocalClientSocket {
    /// Create a new client socket with per-client identity token storage
    /// This is used when multiple clients need to connect to the same server
    pub fn new_with_tokens(
        _client_addr: SocketAddr,
        _server_addr: SocketAddr,
        auth_requests_tx: mpsc::Sender<Vec<u8>>,
        auth_responses_rx: mpsc::Receiver<Vec<u8>>,
        data_tx: mpsc::Sender<Vec<u8>>,
        data_rx: mpsc::Receiver<Vec<u8>>,
        addr_cell: LocalAddrCell,
        identity_token: Arc<Mutex<Option<IdentityToken>>>,
        rejection_code: Arc<Mutex<Option<u16>>>,
    ) -> Self {
        let auth_io = Arc::new(Mutex::new(ClientAuthIo::new(
            auth_responses_rx,
            addr_cell.clone(),
            identity_token,
            rejection_code,
        )));

        Self {
            auth_io,
            sender: LocalClientSender::new(data_tx, addr_cell.clone()),
            receiver: LocalClientReceiver::new(data_rx, addr_cell),
            auth_requests_tx,
        }
    }

    pub fn connect(self) -> (LocalClientIdentity, LocalClientSender, LocalClientReceiver) {
        // Note: connect() without auth doesn't create a PendingRequest.
        // Only connect_with_auth*() methods create it after sending the auth request.
        // This matches the expected behavior - if no auth request is sent, no response is expected.
        let LocalClientSocket {
            auth_io,
            sender,
            receiver,
            ..
        } = self;
        let identity = LocalClientIdentity::new(auth_io);
        (identity, sender, receiver)
    }

    pub fn connect_with_auth(
        self,
        auth_bytes: Vec<u8>,
    ) -> (LocalClientIdentity, LocalClientSender, LocalClientReceiver) {
        // Build HTTP POST request with Authorization header
        let base64_encoded = base64::encode(&auth_bytes);
        let request = http::Request::builder()
            .method("POST")
            .uri("/")
            .header("Authorization", base64_encoded)
            .body(Vec::new())
            .unwrap();

        let request_bytes = naia_shared::transport::request_to_bytes(request);

        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {}

        // Create PendingRequest immediately (not lazily!)
        self.auth_io.lock().unwrap().connect();

        self.connect()
    }

    pub fn connect_with_auth_headers(
        self,
        auth_headers: Vec<(String, String)>,
    ) -> (LocalClientIdentity, LocalClientSender, LocalClientReceiver) {
        // Build HTTP POST request with custom headers
        let mut builder = http::Request::builder().method("POST").uri("/");

        for (key, value) in auth_headers {
            builder = builder.header(key, value);
        }

        let request = builder.body(Vec::new()).unwrap();
        let request_bytes = naia_shared::transport::request_to_bytes(request);

        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {}

        // Create PendingRequest immediately
        self.auth_io.lock().unwrap().connect();

        self.connect()
    }

    pub fn connect_with_auth_and_headers(
        self,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>,
    ) -> (LocalClientIdentity, LocalClientSender, LocalClientReceiver) {
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
        let request_bytes = naia_shared::transport::request_to_bytes(request);

        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {}

        // Create PendingRequest immediately
        self.auth_io.lock().unwrap().connect();

        self.connect()
    }
}
