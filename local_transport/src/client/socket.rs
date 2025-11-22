use std::{net::SocketAddr, sync::{Arc, Mutex}};

use tokio::sync::mpsc;

use crate::shared::LocalTransportQueues;
use super::{
    addr_cell::LocalAddrCell,
    auth::{ClientAuthIo, LocalClientIdentity},
    data::{LocalClientReceiver, LocalClientSender},
};

pub struct LocalClientSocket {
    auth_io: Arc<Mutex<ClientAuthIo>>,
    sender: LocalClientSender,
    receiver: LocalClientReceiver,
    auth_requests_tx: mpsc::UnboundedSender<Vec<u8>>,
}

impl LocalClientSocket {
    pub(crate) fn new(
        shared: LocalTransportQueues,
        _client_addr: SocketAddr,
        _server_addr: SocketAddr,
        auth_requests_tx: mpsc::UnboundedSender<Vec<u8>>,
        auth_responses_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        addr_cell: LocalAddrCell,
    ) -> Self {
        let auth_io = Arc::new(Mutex::new(ClientAuthIo::new(
            auth_responses_rx,
            addr_cell.clone(),
            shared.identity_token.clone(),
            shared.rejection_code.clone(),
        )));

        Self {
            auth_io,
            sender: LocalClientSender::new(shared.client_to_server.clone(), addr_cell.clone()),
            receiver: LocalClientReceiver::new(shared.server_to_client.clone(), addr_cell),
            auth_requests_tx,
        }
    }

    pub fn connect(
        self,
    ) -> (
        LocalClientIdentity,
        LocalClientSender,
        LocalClientReceiver,
    ) {
        let LocalClientSocket { auth_io, sender, receiver, .. } = self;
        let identity = LocalClientIdentity::new(auth_io);
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
        
        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {
            log::trace!("[LocalTransport] Client sent HTTP auth request to server");
        }
        
        // Create PendingRequest immediately (not lazily!)
        self.auth_io.lock().unwrap().connect();
        
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
        
        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {
            log::trace!("[LocalTransport] Client sent HTTP auth request with headers to server");
        }
        
        // Create PendingRequest immediately
        self.auth_io.lock().unwrap().connect();
        
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
        
        // Send to async channel (non-blocking)
        if self.auth_requests_tx.send(request_bytes).is_ok() {
            log::trace!("[LocalTransport] Client sent HTTP auth request with auth and headers to server");
        }
        
        // Create PendingRequest immediately
        self.auth_io.lock().unwrap().connect();
        
        self.connect()
    }
}

