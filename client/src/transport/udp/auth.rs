use std::{
    net::SocketAddr,
    sync::{
        mpsc::{self, TryRecvError},
        Arc, Mutex,
    },
};

use log::warn;

use crate::transport::{udp::addr_cell::AddrCell, IdentityReceiver, IdentityReceiverResult};

pub(crate) struct AuthIo {
    auth_url: String,
    pending_req_opt: Option<PendingRequest>,
    data_addr_cell: AddrCell,
}

impl AuthIo {
    pub(crate) fn new(data_addr_cell: AddrCell, auth_url: &str) -> Self {
        Self {
            auth_url: auth_url.to_string(),
            pending_req_opt: None,
            data_addr_cell,
        }
    }

    pub(crate) fn connect(
        &mut self,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) {
        self.pending_req_opt = Some(PendingRequest::new(
            self.auth_url.clone(),
            auth_bytes_opt,
            auth_headers_opt,
            self.data_addr_cell.clone(),
        ));
    }

    fn receive(&mut self) -> IdentityReceiverResult {
        let Some(pending_req) = self.pending_req_opt.as_mut() else {
            panic!("No stream to receive from (did you forget to call connect?)");
        };
        match pending_req.poll_response() {
            Ok(Some((response_status, id_token))) => {
                if response_status != 200 {
                    return IdentityReceiverResult::ErrorResponseCode(response_status);
                }

                // read the rest of the bytes as the identity token
                IdentityReceiverResult::Success(id_token)
            }
            Ok(None) => IdentityReceiverResult::Waiting,
            Err(HttpError::UreqError(e)) => {
                warn!("Unexpected auth ureq error: {:?}", e);
                IdentityReceiverResult::ErrorResponseCode(500)
            }
            Err(e) => {
                warn!("Unexpected auth read error: {:?}", e);
                IdentityReceiverResult::ErrorResponseCode(500)
            }
        }
    }
}

// AuthReceiver
#[derive(Clone)]
pub(crate) struct AuthReceiver {
    auth_io: Arc<Mutex<AuthIo>>,
}

impl AuthReceiver {
    pub fn new(auth_io: Arc<Mutex<AuthIo>>) -> Self {
        {
            // check if the auth_io is already connected
            let guard = auth_io.lock().unwrap();
            if guard.pending_req_opt.is_none() {
                panic!("AuthReceiver created without a connected AuthIo");
            }
        }

        Self { auth_io }
    }
}

impl IdentityReceiver for AuthReceiver {
    fn receive(&mut self) -> IdentityReceiverResult {
        let mut guard = self.auth_io.lock().unwrap();
        guard.receive()
    }
}

struct PendingRequest {
    receiver: mpsc::Receiver<Result<(u16, String), String>>,
}

impl PendingRequest {
    fn new(
        url: String,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
        addr_cell: AddrCell,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Result<(u16, String), String>>();

        std::thread::spawn(move || {
            let mut request = ureq::post(&url);

            if let Some(auth_bytes) = auth_bytes_opt {
                let base64_encoded = base64::encode(&auth_bytes);
                request = request.set("Authorization", &base64_encoded);
            }
            if let Some(auth_headers) = auth_headers_opt {
                for (key, value) in auth_headers {
                    request = request.set(&key, &value);
                }
            }

            let response_result = match request.call() {
                Ok(response) => {
                    let status_code = response.status();
                    let response_text = match response.into_string() {
                        Ok(text) => text,
                        Err(e) => {
                            let _ = tx.send(Err(format!("Failed to read response body: {}", e)));
                            return;
                        }
                    };

                    let mut response_parts = response_text.splitn(2, "\r\n");
                    let id_token = response_parts.next().unwrap().to_string();
                    let data_addr = response_parts.next().unwrap().to_string();
                    let data_addr: SocketAddr = data_addr.parse().unwrap();
                    // parse out the server's address, put into addrcell
                    addr_cell.recv(&data_addr);

                    Ok((status_code, id_token))
                }
                Err(ureq::Error::Status(code, _response)) => {
                    Ok((code, String::new()))
                }
                Err(e) => Err(format!("{}", e)),
            };
            let _ = tx.send(response_result);
        });

        Self { receiver: rx }
    }

    pub fn poll_response(&mut self) -> Result<Option<(u16, String)>, HttpError> {
        match self.receiver.try_recv() {
            Ok(Ok((status_code, id_token))) => Ok(Some((status_code, id_token))),
            Ok(Err(e)) => Err(HttpError::UreqError(e)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(HttpError::ChannelClosed),
        }
    }
}

#[derive(Debug)]
enum HttpError {
    UreqError(String),
    ChannelClosed,
}
