use std::{
    future,
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};

use log::warn;
use once_cell::sync::Lazy;
use reqwest::header::{HeaderName, HeaderValue};
use tokio::{
    runtime::{Builder, Handle},
    sync::{oneshot, oneshot::error::TryRecvError, RwLock},
};

use crate::transport::{udp::addr_cell::AddrCell, IdentityReceiver, IdentityReceiverResult};

pub(crate) struct AuthIo {
    auth_url: String,
    http_client: Arc<RwLock<reqwest::Client>>,
    pending_req_opt: Option<PendingRequest>,
    data_addr_cell: AddrCell,
}

impl AuthIo {
    pub(crate) fn new(data_addr_cell: AddrCell, auth_url: &str) -> Self {
        let client = reqwest::Client::new();

        Self {
            auth_url: auth_url.to_string(),
            http_client: Arc::new(RwLock::new(client)),
            pending_req_opt: None,
            data_addr_cell,
        }
    }

    pub(crate) fn connect(
        &mut self,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) {
        let mut request =
            reqwest::Request::new(reqwest::Method::POST, self.auth_url.parse().unwrap());
        if let Some(auth_bytes) = auth_bytes_opt {
            let base64_encoded = base64::encode(&auth_bytes);
            let header_name = HeaderName::from_str("Authorization").unwrap();
            let header_value = HeaderValue::from_str(&base64_encoded).unwrap();
            request.headers_mut().insert(header_name, header_value);
        }
        if let Some(auth_headers) = auth_headers_opt {
            let request_headers = request.headers_mut();
            for (key, value) in auth_headers {
                let header_name = HeaderName::from_str(&key).unwrap();
                let header_value = HeaderValue::from_str(&value).unwrap();
                request_headers.insert(header_name, header_value);
            }
        }
        self.pending_req_opt = Some(PendingRequest::new(
            self.http_client.clone(),
            request,
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
            Err(HttpError::ReqwestError(e)) => {
                warn!("Unexpected auth reqwest error: {:?}", e);
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
    receiver: oneshot::Receiver<Result<(u16, String), reqwest::Error>>,
}

impl PendingRequest {
    fn new(
        client: Arc<RwLock<reqwest::Client>>,
        request: reqwest::Request,
        addr_cell: AddrCell,
    ) -> Self {
        let (tx, rx) = oneshot::channel::<Result<(u16, String), reqwest::Error>>();

        get_runtime().spawn(async move {
            let client_guard = client.read().await;

            let response_result = match client_guard.execute(request).await {
                Ok(response) => {
                    let status_code = response.status().as_u16();
                    let response_text = response.text().await.unwrap();

                    let mut response_parts = response_text.splitn(2, "\r\n");
                    let id_token = response_parts.next().unwrap().to_string();
                    let data_addr = response_parts.next().unwrap().to_string();
                    let data_addr: SocketAddr = data_addr.parse().unwrap();
                    // parse out the server's address, put into addrcell
                    addr_cell.recv(&data_addr).await;

                    Ok((status_code, id_token))
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(response_result);
        });

        Self { receiver: rx }
    }

    pub fn poll_response(&mut self) -> Result<Option<(u16, String)>, HttpError> {
        match self.receiver.try_recv() {
            Ok(Ok((status_code, id_token))) => Ok(Some((status_code, id_token))),
            Ok(Err(e)) => Err(HttpError::ReqwestError(e)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Closed) => Err(HttpError::ChannelClosed),
        }
    }
}

#[derive(Debug)]
enum HttpError {
    ReqwestError(reqwest::Error),
    ChannelClosed,
}

fn get_runtime() -> Handle {
    static GLOBAL: Lazy<Handle> = Lazy::new(|| {
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("was not able to build the runtime");

        let runtime_handle = runtime.handle().clone();

        thread::Builder::new()
            .name("tokio-runtime".to_string())
            .spawn(move || {
                let _guard = runtime.enter();
                runtime.block_on(future::pending::<()>());
            })
            .expect("cannot spawn executor thread");

        let _guard = runtime_handle.enter();

        runtime_handle
    });

    Lazy::<Handle>::force(&GLOBAL).clone()
}
