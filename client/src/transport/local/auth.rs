use std::sync::Arc;

use parking_lot::Mutex;

use std::sync::mpsc;

use naia_shared::{
    transport::local::{ClientIdentityReceiverResult, ClientServerAddr, LocalAuthError},
    IdentityToken,
};

use super::addr_cell::LocalAddrCell;

// PendingRequest polls synchronously — no Tokio runtime needed for local transport.
struct PendingRequest {
    auth_responses_rx: mpsc::Receiver<Vec<u8>>,
    addr_cell: LocalAddrCell,
    cached_result: Option<Result<(u16, String), LocalAuthError>>,
}

impl PendingRequest {
    fn new(auth_responses_rx: mpsc::Receiver<Vec<u8>>, addr_cell: LocalAddrCell) -> Self {
        Self {
            auth_responses_rx,
            addr_cell,
            cached_result: None,
        }
    }

    pub fn poll_response(&mut self) -> Result<Option<(u16, String)>, LocalAuthError> {
        // Return cached result if we already parsed the response
        if let Some(ref result) = self.cached_result {
            return match result {
                Ok((status_code, id_token)) => Ok(Some((*status_code, id_token.clone()))),
                Err(e) => Err(e.clone()),
            };
        }

        // Try to receive the auth response synchronously
        let response_bytes = match self.auth_responses_rx.try_recv() {
            Ok(bytes) => bytes,
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(LocalAuthError::ChannelClosed);
            }
        };

        // Parse HTTP response
        let response = naia_shared::transport::bytes_to_response(&response_bytes);
        let status_code = response.status().as_u16();

        if status_code != 200 {
            let result = Ok((status_code, String::new()));
            self.cached_result = Some(result.clone());
            return Ok(Some(result.unwrap()));
        }

        // Parse response body: "identity_token\r\nserver_addr"
        let body = match String::from_utf8(response.body().to_vec()) {
            Ok(b) => b,
            Err(_) => {
                self.cached_result = Some(Err(LocalAuthError::ParseError));
                return Err(LocalAuthError::ParseError);
            }
        };

        let mut parts = body.splitn(2, "\r\n");
        let identity_token = parts.next().unwrap().to_string();
        let server_addr_str = match parts.next() {
            Some(addr) => addr,
            None => {
                self.cached_result = Some(Err(LocalAuthError::ParseError));
                return Err(LocalAuthError::ParseError);
            }
        };

        let server_addr = match server_addr_str.parse() {
            Ok(addr) => addr,
            Err(_) => {
                self.cached_result = Some(Err(LocalAuthError::ParseError));
                return Err(LocalAuthError::ParseError);
            }
        };

        // Update addr_cell synchronously
        self.addr_cell.set(server_addr);

        let result = Ok((status_code, identity_token));
        self.cached_result = Some(result.clone());
        Ok(Some(result.unwrap()))
    }
}

// ClientAuthIo - encapsulates all client auth logic
pub(crate) struct ClientAuthIo {
    auth_responses_rx: Option<mpsc::Receiver<Vec<u8>>>,
    addr_cell: LocalAddrCell,
    pending_req_opt: Option<PendingRequest>,
    identity_token: Arc<Mutex<Option<IdentityToken>>>,
    rejection_code: Arc<Mutex<Option<u16>>>,
}

impl ClientAuthIo {
    pub(crate) fn new(
        auth_responses_rx: mpsc::Receiver<Vec<u8>>,
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
        let auth_responses_rx = self
            .auth_responses_rx
            .take()
            .expect("auth_responses_rx already taken");

        self.pending_req_opt = Some(PendingRequest::new(
            auth_responses_rx,
            self.addr_cell.clone(),
        ));
    }

    fn receive(&mut self) -> ClientIdentityReceiverResult {
        // Check if already received token (from previous call)
        if let Some(token) = self.identity_token.lock().clone() {
            return ClientIdentityReceiverResult::Success(token);
        }

        // Check if rejection happened
        if let Some(code) = *self.rejection_code.lock() {
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
                    *self.rejection_code.lock() = Some(status_code);
                    return ClientIdentityReceiverResult::ErrorResponseCode(status_code);
                }

                // Verify address is available before returning Success
                match self.addr_cell.get() {
                    ClientServerAddr::Finding => {
                        return ClientIdentityReceiverResult::Waiting;
                    }
                    ClientServerAddr::Found(_addr) => {}
                }

                *self.identity_token.lock() = Some(id_token.clone());
                ClientIdentityReceiverResult::Success(id_token)
            }
            Ok(None) => ClientIdentityReceiverResult::Waiting,
            Err(_e) => ClientIdentityReceiverResult::ErrorResponseCode(500),
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
        self.auth_io.lock().receive()
    }
}
