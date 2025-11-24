use std::sync::{Arc, Mutex};

use log::warn;


use tokio::sync::{mpsc, oneshot, oneshot::error::TryRecvError};

use naia_shared::IdentityToken;

use naia_shared::transport::local::{get_runtime, ClientIdentityReceiverResult, ClientServerAddr, LocalAuthError};

use super::addr_cell::LocalAddrCell;

// PendingRequest for async auth handling
struct PendingRequest {
    receiver: oneshot::Receiver<Result<(u16, String), LocalAuthError>>,
}

impl PendingRequest {
    fn new(
        mut auth_responses_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        addr_cell: LocalAddrCell,
    ) -> Self {
        let (tx, rx) = oneshot::channel::<Result<(u16, String), LocalAuthError>>();

        get_runtime().spawn(async move {
            // Wait for auth response from mpsc channel (one message)
            // Since we own the receiver, no mutex needed!
            let response_bytes = match auth_responses_rx.recv().await {
                Some(bytes) => bytes,
                None => {
                    // Channel closed
                    let _ = tx.send(Err(LocalAuthError::ChannelClosed));
                    return;
                }
            };

            // Parse HTTP response
            let response = naia_shared::transport::bytes_to_response(&response_bytes);
            let status_code = response.status().as_u16();
            
            if status_code != 200 {
                let _ = tx.send(Ok((status_code, String::new())));
                return;
            }

            // Parse response body: "identity_token\r\nserver_addr"
            let body = match String::from_utf8(response.body().to_vec()) {
                Ok(b) => b,
                Err(_) => {
                    let _ = tx.send(Err(LocalAuthError::ParseError));
                    return;
                }
            };
            
            let mut parts = body.splitn(2, "\r\n");
            let identity_token = parts.next().unwrap().to_string();
            let server_addr_str = match parts.next() {
                Some(addr) => addr,
                None => {
                    let _ = tx.send(Err(LocalAuthError::ParseError));
                    return;
                }
            };
            
            let server_addr = match server_addr_str.parse() {
                Ok(addr) => addr,
                Err(_) => {
                    let _ = tx.send(Err(LocalAuthError::ParseError));
                    return;
                }
            };
            
            // Update addr_cell asynchronously
            // IMPORTANT: Update addr_cell BEFORE sending result so client can use it immediately
            addr_cell.recv(server_addr).await;
            log::trace!("[LocalTransport] Updated addr_cell with server address: {}", server_addr);
            
            let _ = tx.send(Ok((status_code, identity_token)));
        });

        Self { receiver: rx }
    }

    pub fn poll_response(&mut self) -> Result<Option<(u16, String)>, LocalAuthError> {
        match self.receiver.try_recv() {
            Ok(Ok((status_code, id_token))) => Ok(Some((status_code, id_token))),
            Ok(Err(e)) => Err(e),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(LocalAuthError::ChannelClosed),
        }
    }
}

// ClientAuthIo - encapsulates all client auth logic
pub(crate) struct ClientAuthIo {
    auth_responses_rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    addr_cell: LocalAddrCell,
    pending_req_opt: Option<PendingRequest>,
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
}

impl ClientAuthIo {
    pub(crate) fn new(
        auth_responses_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        addr_cell: LocalAddrCell,
        identity_token: Arc<Mutex<Option<IdentityToken>>>,
        rejection_code: Arc<Mutex<Option<u16>>>,
    ) -> Self {
        Self {
            auth_responses_rx: Some(auth_responses_rx),
            addr_cell,
            pending_req_opt: None,
            identity_token,
            rejection_code,
        }
    }
    
    // Called by LocalClientSocket during connect
    pub(crate) fn connect(&mut self) {
        // Create PendingRequest immediately (not lazily!) if one doesn't exist
        if self.pending_req_opt.is_some() {
            // Already created, skip
            return;
        }
        
        // Take ownership of the receiver
        let auth_responses_rx = self.auth_responses_rx.take()
            .expect("auth_responses_rx already taken");
        
        self.pending_req_opt = Some(PendingRequest::new(
            auth_responses_rx,
            self.addr_cell.clone(),
        ));
        log::trace!("[LocalTransport] Client created PendingRequest for auth");
    }
    
    fn receive(&mut self) -> ClientIdentityReceiverResult {
        // Check if already received token (from previous call)
        if let Some(token) = self.identity_token.lock().unwrap().clone() {
            log::trace!("[LocalTransport] Client identity receiver: Success(token={})", token);
            return ClientIdentityReceiverResult::Success(token);
        }
        
        // Check if rejection happened
        if let Some(code) = *self.rejection_code.lock().unwrap() {
            log::trace!("[LocalTransport] Client identity receiver: ErrorResponseCode({})", code);
            return ClientIdentityReceiverResult::ErrorResponseCode(code);
        }
        
        // Check if we have a pending request
        if self.pending_req_opt.is_none() {
            panic!("No PendingRequest (did you forget to call connect?)");
        }
        
        // Poll the pending request
        let pending_req = self.pending_req_opt.as_mut().unwrap();
        match pending_req.poll_response() {
            Ok(Some((status_code, id_token))) => {
                if status_code != 200 {
                    *self.rejection_code.lock().unwrap() = Some(status_code);
                    log::trace!("[LocalTransport] Client identity receiver: ErrorResponseCode({})", status_code);
                    return ClientIdentityReceiverResult::ErrorResponseCode(status_code);
                }
                
                // Verify address is available before returning Success
                match self.addr_cell.get() {
                    ClientServerAddr::Finding => {
                        log::trace!("[LocalTransport] Address not yet available, still waiting...");
                        return ClientIdentityReceiverResult::Waiting;
                    }
                    ClientServerAddr::Found(addr) => {
                        log::trace!("[LocalTransport] Address available: {}", addr);
                    }
                }
                
                *self.identity_token.lock().unwrap() = Some(id_token.clone());
                log::trace!("[LocalTransport] Client identity receiver: Success(token={})", id_token);
                ClientIdentityReceiverResult::Success(id_token)
            }
            Ok(None) => {
                log::trace!("[LocalTransport] Client identity receiver: Still waiting...");
                ClientIdentityReceiverResult::Waiting
            }
            Err(e) => {
                warn!("[LocalTransport] Unexpected auth error: {:?}", e);
                ClientIdentityReceiverResult::ErrorResponseCode(500)
            }
        }
    }
}

// LocalClientIdentity wraps Arc<Mutex<ClientAuthIo>>
#[derive(Clone)]
pub struct LocalClientIdentity {
    auth_io: Arc<Mutex<ClientAuthIo>>,
}

impl LocalClientIdentity {
    pub(crate) fn new(auth_io: Arc<Mutex<ClientAuthIo>>) -> Self {
        Self { auth_io }
    }
    
    pub fn receive(&mut self) -> ClientIdentityReceiverResult {
        self.auth_io.lock().unwrap().receive()
    }
}

