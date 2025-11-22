use std::{net::SocketAddr, sync::{Arc, Mutex}};

use naia_shared::IdentityToken;
use tokio::sync::mpsc;

use crate::shared::{ServerRecvError, ServerSendError, FAKE_CLIENT_ADDR};

// ServerAuthIo - encapsulates all server auth logic  
pub(crate) struct ServerAuthIo {
    auth_requests_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    auth_responses_tx: mpsc::UnboundedSender<Vec<u8>>,
    server_data_addr: SocketAddr,
    buffer: [u8; 1472],
    client_addr: SocketAddr,
}

impl ServerAuthIo {
    pub(crate) fn new(
        auth_requests_rx: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
        auth_responses_tx: mpsc::UnboundedSender<Vec<u8>>,
        server_data_addr: SocketAddr,
    ) -> Self {
        let client_addr: SocketAddr = FAKE_CLIENT_ADDR.parse().expect("invalid client addr");
        Self {
            auth_requests_rx,
            auth_responses_tx,
            server_data_addr,
            buffer: [0; 1472],
            client_addr,
        }
    }
    
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        // Try to receive from async channel (non-blocking)
        let mut rx_guard = self.auth_requests_rx.lock().unwrap();
        if let Ok(request_bytes) = rx_guard.try_recv() {
            log::trace!("[LocalTransport] Server received HTTP auth request");
            
            // Parse HTTP request
            let request = naia_shared::http_utils::bytes_to_request(&request_bytes);
            
            // Extract Authorization header if present
            if let Some(auth_header) = request.headers().get("Authorization") {
                let auth_str = auth_header.to_str().unwrap();
                let auth_bytes = base64::decode(auth_str).unwrap();
                let len = auth_bytes.len();
                self.buffer[0..len].copy_from_slice(&auth_bytes);
                Ok(Some((self.client_addr, &self.buffer[..len])))
            } else {
                // No auth header present, return empty auth (for connect_with_auth_headers case)
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
    
    fn accept(
        &mut self,
        _address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), ServerSendError> {
        // Build HTTP 200 response with identity token and server address in body
        let response_body = format!("{}\r\n{}", identity_token, self.server_data_addr);
        let response = http::Response::builder()
            .status(200)
            .body(response_body.into_bytes())
            .unwrap();
        
        let response_bytes = naia_shared::http_utils::response_to_bytes(response);
        
        // Send to mpsc channel (non-blocking)
        self.auth_responses_tx.send(response_bytes)
            .map_err(|_| ServerSendError)?;
        log::debug!("[LocalTransport] Server sent HTTP 200 response with identity token");
        
        Ok(())
    }

    fn reject(&mut self, _address: &SocketAddr) -> Result<(), ServerSendError> {
        // Build HTTP 401 response
        let response = http::Response::builder()
            .status(401)
            .body(Vec::new())
            .unwrap();
        
        let response_bytes = naia_shared::http_utils::response_to_bytes(response);
        
        // Send to mpsc channel (non-blocking)
        self.auth_responses_tx.send(response_bytes)
            .map_err(|_| ServerSendError)?;
        log::debug!("[LocalTransport] Server sent HTTP 401 rejection response");
        
        Ok(())
    }
}

// LocalServerAuthSender wraps Arc<Mutex<ServerAuthIo>>
#[derive(Clone)]
pub struct LocalServerAuthSender {
    auth_io: Arc<Mutex<ServerAuthIo>>,
}

impl LocalServerAuthSender {
    pub(crate) fn new(auth_io: Arc<Mutex<ServerAuthIo>>) -> Self {
        Self { auth_io }
    }

    pub fn accept(&self, address: &SocketAddr, identity_token: &IdentityToken) -> Result<(), ServerSendError> {
        self.auth_io.lock().unwrap().accept(address, identity_token)
    }

    pub fn reject(&self, address: &SocketAddr) -> Result<(), ServerSendError> {
        self.auth_io.lock().unwrap().reject(address)
    }
}

// LocalServerAuthReceiver wraps Arc<Mutex<ServerAuthIo>> with its own buffer
#[derive(Clone)]
pub struct LocalServerAuthReceiver {
    auth_io: Arc<Mutex<ServerAuthIo>>,
    buffer: Box<[u8]>,
}

impl LocalServerAuthReceiver {
    pub(crate) fn new(auth_io: Arc<Mutex<ServerAuthIo>>) -> Self {
        Self {
            auth_io,
            buffer: Box::new([0; 1472]),
        }
    }

    pub fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, ServerRecvError> {
        let mut guard = self.auth_io.lock().unwrap();
        match guard.receive() {
            Ok(option) => match option {
                Some((addr, buffer)) => {
                    self.buffer = buffer.into();
                    Ok(Some((addr, &self.buffer)))
                }
                None => Ok(None),
            },
            Err(err) => Err(err),
        }
    }
}

