use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    pin::Pin,
    task::{Context, Poll},
};

use async_dup::Arc;
use futures_core::Stream;
use http::{header, HeaderValue, Response};
use log::{info, warn};
use smol::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, Lines},
    lock::Mutex,
    stream::StreamExt,
    Async,
};
use webrtc_unreliable::SessionEndpoint;

use naia_socket_shared::{
    SocketConfig, PROTOCOL_ID_HEADER, PROTOCOL_ID_HEADER_VALUE_LEN, PROTOCOL_MISMATCH_STATUS,
};

use crate::{executor, server_addrs::ServerAddrs, AuthResponse, NaiaServerSocketError};

/// Caps on what an unauthenticated client may make the session listener buffer.
///
/// `serve` reads the session request before any authentication happens, so every
/// byte it accumulates is attacker-controlled. Without these caps a single peer
/// can hold the connection open and stream an unterminated header line, or
/// declare an enormous `Content-Length`, until the server exhausts memory.
/// The values are far above any legitimate SDP session request.
const MAX_REQUEST_LINE_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Value of the CORS `Access-Control-Allow-Headers` response header.
///
/// A browser will not send the POST at all unless the preflight response names
/// every custom header the request carries, so the protocol-fingerprint header
/// has to appear here or the wasm backends cannot connect. It is a literal
/// because `HeaderValue::from_static` requires one; a test in this module
/// asserts it still contains [`PROTOCOL_ID_HEADER`], so renaming the header
/// without updating this string fails the build's tests rather than breaking
/// browsers at runtime.
const CORS_ALLOW_HEADERS: &str = "Authorization, Content-Length, X-Naia-Protocol-Id";

type ClientAuthSender =
    smol::channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;

type AuthMuxMap = Arc<
    Mutex<
        HashMap<
            SocketAddr,
            (
                Option<futures_channel::oneshot::Sender<AuthResponse>>,
                Option<AuthResponse>,
            ),
        >,
    >,
>;

/// The request-line prefixes this listener accepts, derived from the
/// SocketConfig. Per-listener (not global) so multiple server Sockets can
/// coexist in one process (e.g. tests).
#[derive(Clone)]
struct RtcUrlPaths {
    post: String,
    options: String,
}

pub fn start_session_server(
    server_addrs: ServerAddrs,
    config: SocketConfig,
    session_endpoint: SessionEndpoint,
    from_client_auth_sender: Option<ClientAuthSender>,
    to_session_all_auth_receiver: Option<smol::channel::Receiver<(SocketAddr, AuthResponse)>>,
    expected_protocol_id: String,
) {
    executor::spawn(async move {
        listen(
            server_addrs,
            config,
            session_endpoint.clone(),
            from_client_auth_sender,
            to_session_all_auth_receiver,
            expected_protocol_id,
        )
        .await;
    })
    .detach();
}

/// Listens for incoming connections and serves them.
async fn listen(
    server_addrs: ServerAddrs,
    config: SocketConfig,
    session_endpoint: SessionEndpoint,
    from_client_auth_sender: Option<ClientAuthSender>,
    to_session_all_auth_receiver: Option<smol::channel::Receiver<(SocketAddr, AuthResponse)>>,
    expected_protocol_id: String,
) {
    let rtc_url_paths = RtcUrlPaths {
        post: format!("POST /{}", config.rtc_endpoint_path),
        options: format!("OPTIONS /{}", config.rtc_endpoint_path),
    };
    let socket_address = server_addrs.session_listen_addr;

    let listener = Async::<TcpListener>::bind(socket_address)
        .expect("unable to bind a TCP Listener to the supplied socket address");
    info!(
        "Session initiator available at POST http://{}/{}",
        listener
            .get_ref()
            .local_addr()
            .expect("Listener does not have a local address"),
        config.rtc_endpoint_path
    );

    let mut auth_mux_sender_opt =
        if let Some(to_session_all_auth_receiver) = to_session_all_auth_receiver {
            Some(setup_auth_mux(to_session_all_auth_receiver).await)
        } else {
            None
        };

    loop {
        // Accept the next connection.
        let (response_stream, remote_addr) = listener
            .accept()
            .await
            .expect("was not able to accept the incoming stream from the listener");

        let session_endpoint_clone = session_endpoint.clone();

        let (to_session_single_auth_sender, to_session_single_auth_receiver) =
            if from_client_auth_sender.is_some() {
                let (sender, receiver) = futures_channel::oneshot::channel();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
        if let Some(to_session_single_auth_sender) = to_session_single_auth_sender {
            let result = auth_mux_sender_opt
                .as_mut()
                .unwrap()
                .send((remote_addr, to_session_single_auth_sender))
                .await;
            if result.is_err() {
                warn!("Unable to send auth sender to auth mux");
                continue;
            }
        }

        let from_client_auth_sender = from_client_auth_sender.clone();
        let rtc_url_paths = rtc_url_paths.clone();
        let expected_protocol_id = expected_protocol_id.clone();
        // Spawn a background task serving this connection.
        executor::spawn(async move {
            serve(
                session_endpoint_clone,
                Arc::new(response_stream),
                from_client_auth_sender,
                to_session_single_auth_receiver,
                rtc_url_paths,
                expected_protocol_id,
            )
            .await;
        })
        .detach();
    }
}

async fn setup_auth_mux(
    to_session_all_auth_receiver: smol::channel::Receiver<(SocketAddr, AuthResponse)>,
) -> smol::channel::Sender<(SocketAddr, futures_channel::oneshot::Sender<AuthResponse>)> {
    let (sender_sender, sender_receiver) = smol::channel::unbounded();

    let map_1 = Arc::new(Mutex::new(HashMap::new()));
    let map_2 = map_1.clone();

    // Spawn a background task for muxing in
    executor::spawn(async move {
        serve_auth_mux_in(map_1, to_session_all_auth_receiver).await;
    })
    .detach();

    // Spawn a background task for muxing out
    executor::spawn(async move {
        serve_auth_mux_out(map_2, sender_receiver).await;
    })
    .detach();

    sender_sender
}

async fn serve_auth_mux_in(
    map: AuthMuxMap,
    to_session_all_auth_receiver: smol::channel::Receiver<(SocketAddr, AuthResponse)>,
) {
    loop {
        let Ok((addr, answer)) = to_session_all_auth_receiver.recv().await else {
            // Channel closed: the server Socket is gone; end this task
            // (continuing would busy-loop forever on a closed channel).
            return;
        };

        // info!("received auth answer from app, for addr: {}, answer: {:?}", addr, answer);

        let mut map = map.lock().await;
        if let Some((Some(_), _)) = map.get(&addr) {
            // info!("auth answer sender exists for: {}", addr);
            let sender = map.remove(&addr).unwrap().0.unwrap();
            // info!("sending auth answer to session: {}", addr);
            if sender.send(answer).is_err() {
                warn!("Unable to send auth to session");
                continue;
            }
        } else {
            // info!("auth answer sender does not exist for: {}, inserting answer", addr);
            map.insert(addr, (None, Some(answer)));
        }
    }
}

async fn serve_auth_mux_out(
    map: AuthMuxMap,
    sender_receiver: smol::channel::Receiver<(
        SocketAddr,
        futures_channel::oneshot::Sender<AuthResponse>,
    )>,
) {
    loop {
        let Ok((addr, sender)) = sender_receiver.recv().await else {
            // Channel closed: the listener is gone; end this task.
            return;
        };

        // info!("received auth answer sender, for addr: {}", addr);

        let mut map = map.lock().await;
        if let Some((_, Some(_))) = map.get(&addr) {
            // info!("auth answer exists for: {}", addr);
            let (_, Some(answer)) = map.remove(&addr).unwrap() else {
                panic!("shouldn't be possible");
            };
            // info!("sending auth answer to session: {}", addr);
            if sender.send(answer).is_err() {
                warn!("Unable to send auth to session");
                continue;
            }
        } else {
            // info!("auth answer does not exist for: {}, inserting sender", addr);
            map.insert(addr, (Some(sender), None));
        }
    }
}

/// A session request that was read to completion before authentication.
struct SessionRequest {
    is_options: bool,
    auth_string: Option<String>,
    /// The peer's protocol fingerprint, lowercased, and only if it was exactly
    /// [`PROTOCOL_ID_HEADER_VALUE_LEN`] characters long.
    ///
    /// A header that was absent, and one whose value was the wrong width, both
    /// land here as `None`. That is deliberate: the gate in `serve` must not
    /// be able to tell those cases apart, and collapsing them at the parser
    /// means no later code can accidentally reintroduce the distinction.
    protocol_id: Option<String>,
    body: Vec<u8>,
}

/// Reads one HTTP session request off `reader`.
///
/// This runs entirely pre-authentication, so every byte it sees is chosen by an
/// unauthenticated remote peer. Anything malformed -- an I/O error, a non-UTF-8
/// header line, an over-long line, over-long headers, or an over-large declared
/// `Content-Length` -- yields `None` so the caller can answer 404 and drop the
/// connection. None of it may panic: `serve` runs one task per incoming
/// connection, and a panic there takes the whole server process down.
async fn read_session_request<R: AsyncRead + Unpin>(
    reader: R,
    rtc_url_paths: &RtcUrlPaths,
    remote_addr: &SocketAddr,
) -> Option<SessionRequest> {
    let mut bytes = reader.bytes();

    let mut headers_been_read: bool = false;
    let mut content_length: Option<usize> = None;
    let mut auth_string: Option<String> = None;
    let mut protocol_id: Option<String> = None;
    let protocol_id_prefix = format!("{}: ", PROTOCOL_ID_HEADER);
    let mut rtc_url_matched = false;
    let mut is_options: bool = false;
    let mut body: Vec<u8> = Vec::new();

    let mut line: Vec<u8> = Vec::new();
    let mut header_bytes_read: usize = 0;

    while let Some(byte) = bytes.next().await {
        let byte = match byte {
            Ok(byte) => byte,
            Err(err) => {
                warn!(
                    "Error reading WebRTC session request from {}: {}",
                    remote_addr, err
                );
                return None;
            }
        };

        if !headers_been_read {
            header_bytes_read += 1;
            if header_bytes_read > MAX_HEADER_BYTES {
                warn!(
                    "Over-long headers in WebRTC session request from {}",
                    remote_addr
                );
                return None;
            }
        }

        if headers_been_read {
            if let Some(content_length) = content_length {
                body.push(byte);

                if body.len() >= content_length {
                    return Some(SessionRequest {
                        is_options,
                        auth_string,
                        protocol_id,
                        body,
                    });
                }
            } else {
                info!("request was missing Content-Length header");
                return None;
            }
        }

        if byte == b'\r' {
            continue;
        } else if byte == b'\n' {
            // Header lines come straight off the wire pre-auth; non-UTF-8 is a
            // malformed request, not a server fault.
            let Ok(mut str) = String::from_utf8(line.clone()) else {
                warn!(
                    "Non-UTF-8 header line in WebRTC session request from {}",
                    remote_addr
                );
                return None;
            };
            line.clear();

            if rtc_url_matched {
                if str.to_lowercase().starts_with("content-length: ") {
                    let (_, last) = str.split_at(16);
                    str = last.to_string();
                    content_length = str.parse::<usize>().ok();
                    if content_length.is_some_and(|len| len > MAX_BODY_BYTES) {
                        warn!(
                            "Over-large Content-Length in WebRTC session request from {}",
                            remote_addr
                        );
                        return None;
                    }
                } else if str.to_lowercase().starts_with("authorization: ") {
                    let (_, last) = str.split_at(15);
                    auth_string = Some(last.to_string());
                } else if let Some(value) = str.to_lowercase().strip_prefix(&protocol_id_prefix) {
                    // Shape is checked here, not at the gate, so that a
                    // truncated, padded or non-hex value becomes
                    // indistinguishable from an absent one before anything
                    // downstream can see it. The line has already been
                    // lowercased, so a fingerprint that arrived in upper case
                    // is accepted and normalised rather than refused.
                    let well_formed = value.len() == PROTOCOL_ID_HEADER_VALUE_LEN
                        && value.bytes().all(|byte| byte.is_ascii_hexdigit());
                    if well_formed {
                        protocol_id = Some(value.to_string());
                    }
                } else if str.is_empty() {
                    headers_been_read = true;

                    if is_options {
                        return Some(SessionRequest {
                            is_options,
                            auth_string,
                            protocol_id,
                            body,
                        });
                    }
                }
            } else if str.starts_with(&rtc_url_paths.post) {
                rtc_url_matched = true;
            } else if str.starts_with(&rtc_url_paths.options) {
                rtc_url_matched = true;
                is_options = true;
            }
        } else {
            if line.len() >= MAX_REQUEST_LINE_BYTES {
                warn!(
                    "Over-long header line in WebRTC session request from {}",
                    remote_addr
                );
                return None;
            }
            line.push(byte);
        }
    }

    // Stream ended before the request was complete.
    None
}

/// The fingerprint decision, alone.
///
/// Lifted out of [`serve`] because that function is an async handler wrapped
/// around a live TCP stream: the branch cannot be exercised there without
/// standing up the whole listener, and a gate asserted only through its
/// surroundings is a gate that can be quietly widened. Here it is a pure
/// function of the three inputs it depends on, so "an absent header passes" is
/// directly falsifiable.
///
/// `is_options` is the one exemption: a CORS preflight carries no custom
/// headers by construction, and the POST that follows it is gated.
fn fingerprint_is_acceptable(protocol_id: Option<&str>, expected: &str, is_options: bool) -> bool {
    if is_options {
        return true;
    }
    protocol_id == Some(expected)
}

/// Reads a request from the client and sends it a response.
async fn serve(
    mut session_endpoint: SessionEndpoint,
    mut stream: Arc<Async<TcpStream>>,
    from_client_auth_sender: Option<ClientAuthSender>,
    to_session_single_auth_receiver: Option<futures_channel::oneshot::Receiver<AuthResponse>>,
    rtc_url_paths: RtcUrlPaths,
    expected_protocol_id: String,
) {
    // A peer that vanishes between accept() and here leaves us without an
    // address; that is a normal remote event, not a server fault.
    let Ok(remote_addr) = stream.get_ref().peer_addr() else {
        warn!("Incoming WebRTC session request has no peer address, dropping");
        return;
    };

    info!("Incoming WebRTC session request from {}", remote_addr);

    // Parse the request before any authentication has happened: everything this
    // reads is attacker-controlled, so it must never panic and must never buffer
    // without bound. `None` means the request was malformed or over-large.
    let request =
        read_session_request(BufReader::new(stream.clone()), &rtc_url_paths, &remote_addr).await;
    let (mut success, is_options, auth_string, protocol_id, body) = match request {
        Some(request) => (
            true,
            request.is_options,
            request.auth_string,
            request.protocol_id,
            request.body,
        ),
        None => (false, false, None, None, Vec::new()),
    };

    // Protocol-fingerprint gate.
    //
    // This is the first thing checked about a real session request, and it is
    // checked *before* the credential is base64-decoded, before it is handed to
    // the app, and before any per-peer state is created. A peer running a
    // different protocol is refused here without ever reaching the auth path.
    //
    // Absent, wrong-width and wrong-value all arrive as one condition and take
    // one branch, and the answer is the shared reserved mismatch status with
    // an empty body -- never the 404 a malformed request gets, never the 401
    // an application rejection gets, and never carrying either fingerprint.
    // The expected value is never echoed: it is public compatibility metadata,
    // not a secret, but echoing it would turn this into an oracle that hands
    // any peer the value it failed to supply.
    //
    // OPTIONS is exempt because a CORS preflight carries no custom headers by
    // construction; the POST that follows it is gated.
    let mut fingerprint_mismatch = false;
    if success
        && !fingerprint_is_acceptable(protocol_id.as_deref(), &expected_protocol_id, is_options)
    {
        warn!(
            "Refusing WebRTC session request from {}: protocol fingerprint mismatch",
            remote_addr
        );
        success = false;
        fingerprint_mismatch = true;
    }
    let mut identity_token_opt = None;
    // Optional serialized message explaining a rejection (naia-lib/naia#133).
    let mut reject_payload_opt: Option<Vec<u8>> = None;

    {
        // handle OPTIONS request
        if success && is_options {
            let mut resp = Response::<String>::new("".to_string());
            resp.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            );
            resp.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("POST"),
            );
            // The fingerprint header is a custom request header, so a browser
            resp.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static(CORS_ALLOW_HEADERS),
            );
            resp.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );

            let mut out = response_header_to_vec(&resp);
            out.extend_from_slice(resp.body().as_bytes());

            // info!("OPTIONS request from {}", remote_addr);

            if stream.write_all(&out).await.is_err() {
                warn!("Error writing response to {}", remote_addr);
                return;
            }
        }

        // handle auth
        if success && !is_options {
            if let Some(from_client_auth_sender) = from_client_auth_sender {
                success = false;

                let to_session_auth_receiver = to_session_single_auth_receiver.unwrap();

                // check auth
                if let Some(auth_string) = auth_string {
                    match base64::decode(&auth_string) {
                        Ok(decoded_bytes) => {
                            if from_client_auth_sender
                                .send(Ok((remote_addr, decoded_bytes.into())))
                                .await
                                .is_err()
                            {
                                warn!("Unable to send auth string to server app");
                            } else {
                                // info!("Sent auth bytes to server app");

                                // wait for response from app
                                if let Ok(auth_response) = to_session_auth_receiver.await {
                                    match auth_response {
                                        AuthResponse::Accept(identity_token) => {
                                            // info!("Server app accepted auth with identity token: {}", identity_token);
                                            identity_token_opt = Some(identity_token);
                                            success = true;
                                        }
                                        AuthResponse::Reject(payload) => {
                                            // warn!("Server app rejected auth");
                                            identity_token_opt = None;
                                            reject_payload_opt = payload;
                                            success = true;
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            warn!("Invalid WebRTC session request from {}. Error: unable to decode auth string", remote_addr);
                        }
                    }
                } else {
                    warn!(
                        "Invalid WebRTC session request from {}. Error: missing auth string",
                        remote_addr
                    );
                }
            } else {
                warn!(
                    "Invalid WebRTC session request from {}. Error: missing auth sender",
                    remote_addr
                );
            }
        }

        // read body and init session
        if success && !is_options {
            success = false;

            // info!("reading identity token");

            if let Some(identity_token) = identity_token_opt.take() {
                // info!("identity token: {:?}", identity_token);

                let mut lines = body.lines();
                let buf = RequestBuffer::new(&mut lines);

                match session_endpoint.http_session_request(buf).await {
                    Ok(resp) => {
                        // info!("Successful WebRTC session request");

                        success = true;

                        let (_head, body) = resp.into_parts();

                        let identity_token_string = identity_token.to_signaling_string();
                        let body = format!(
                            "{{\
                        \"sdp\":{body},\
                        \"id\":\"{identity_token_string}\"\
                        }}",
                        );

                        let response = Response::builder()
                            .header(header::CONTENT_TYPE, "application/json")
                            .header(
                                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                                HeaderValue::from_static("*"),
                            )
                            .body(body)
                            .expect("could not combine sdp response with id token");

                        let mut out = response_header_to_vec(&response);
                        out.extend_from_slice(response.body().as_bytes());

                        info!("Successful WebRTC session request from {}", remote_addr);

                        if stream.write_all(&out).await.is_err() {
                            warn!("Error writing response to {}", remote_addr);
                            return;
                        }
                    }
                    Err(err) => {
                        warn!(
                            "Invalid WebRTC session request from {}. Error: {}",
                            remote_addr, err
                        );
                    }
                }
            } else {
                // Server rejected auth!
                // The rejection message rides base64-encoded in the body, so
                // that a text-bodied response can carry arbitrary message bits.
                let reject_body = match &reject_payload_opt {
                    Some(bytes) => base64::encode(bytes),
                    None => String::new(),
                };
                let response = Response::builder()
                    .status(401)
                    .header(header::CONTENT_LENGTH, reject_body.len())
                    .body(reject_body)
                    .expect("could not build 401 response");

                let mut out = response_header_to_vec(&response);
                out.extend_from_slice(response.body().as_bytes());

                // A rejection is a complete answer. Without this the 404
                // written below is appended to it on the same connection, and
                // a client reading to EOF sees the two responses run together.
                success = true;

                info!("Rejected WebRTC session request from {}", remote_addr);

                if stream.write_all(&out).await.is_err() {
                    warn!("Error writing response to {}", remote_addr);
                    return;
                }
            }
        }
    }

    // info!("Closing WebRTC session request from {}", remote_addr);

    // From here on the peer may already be gone; a failed write/flush/close is a
    // remote event, so log it rather than taking the whole server down.
    // A fingerprint mismatch gets the shared reserved mismatch status with an
    // empty body (built from the constant, so the three transports cannot
    // drift); malformed framing still gets the generic 404.
    if !success {
        if fingerprint_mismatch {
            let mismatch = Response::builder()
                .status(PROTOCOL_MISMATCH_STATUS)
                .header(header::CONTENT_LENGTH, 0)
                .header(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    HeaderValue::from_static("*"),
                )
                .body(Vec::<u8>::new())
                .expect("could not build protocol-mismatch response");
            let out = response_header_to_vec(&mismatch);
            if stream.write_all(&out).await.is_err() {
                warn!("Error writing mismatch response to {}", remote_addr);
                return;
            }
        } else if stream.write_all(RESPONSE_BAD).await.is_err() {
            warn!("Error writing 404 response to {}", remote_addr);
            return;
        }
    }

    if stream.flush().await.is_err() {
        warn!("Error flushing stream to {}", remote_addr);
        return;
    }
    if stream.close().await.is_err() {
        warn!("Error closing stream to {}", remote_addr);
    }
}

const RESPONSE_BAD: &[u8] = br#"
HTTP/1.1 404 NOT FOUND
Content-Type: text/html
Content-Length: 0
Access-Control-Allow-Origin: *
"#;

struct RequestBuffer<'a, R: AsyncBufRead + Unpin> {
    buffer: &'a mut Lines<R>,
    add_newline: bool,
}

impl<'a, R: AsyncBufRead + Unpin> RequestBuffer<'a, R> {
    fn new(buf: &'a mut Lines<R>) -> Self {
        RequestBuffer {
            add_newline: false,
            buffer: buf,
        }
    }
}

type ReqError = std::io::Error; //Box<dyn error::Error + Send + Sync>;

const NEWLINE_STR: &str = "\n";

impl<'a, R: AsyncBufRead + Unpin> Stream for RequestBuffer<'a, R> {
    type Item = Result<String, ReqError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.add_newline {
            self.add_newline = false;
            Poll::Ready(Some(Ok(String::from(NEWLINE_STR))))
        } else {
            // R: Unpin means Lines<R>: Unpin and &mut Lines<R>: Unpin, so Pin::new is safe.
            let mut_ref = Pin::new(&mut self.buffer);
            match Stream::poll_next(mut_ref, cx) {
                Poll::Ready(Some(item)) => {
                    self.add_newline = true;
                    Poll::Ready(Some(item))
                }
                Poll::Ready(None) => Poll::Ready(None),
                // The underlying reader is always a fully-buffered &[u8] (Vec<u8> read to
                // completion before RequestBuffer is created), so Pending is unreachable.
                Poll::Pending => unreachable!("in-memory &[u8] reader never yields Pending"),
            }
        }
    }
}

fn response_header_to_vec<T>(r: &Response<T>) -> Vec<u8> {
    let v = Vec::with_capacity(120);
    let mut c = std::io::Cursor::new(v);
    write_response_header(r, &mut c).expect("unable to write response header to stream");
    c.into_inner()
}

fn write_response_header<T>(
    r: &Response<T>,
    mut io: impl std::io::Write,
) -> std::io::Result<usize> {
    let mut len = 0;
    macro_rules! w {
        ($x:expr) => {
            io.write_all($x)?;
            len += $x.len();
        };
    }

    let status = r.status();
    let code = status.as_str();
    let reason = status.canonical_reason().unwrap_or("Unknown");
    let headers = r.headers();

    w!(b"HTTP/1.1 ");
    w!(code.as_bytes());
    w!(b" ");
    w!(reason.as_bytes());
    w!(b"\r\n");

    for (hn, hv) in headers {
        w!(hn.as_str().as_bytes());
        w!(b": ");
        w!(hv.as_bytes());
        w!(b"\r\n");
    }

    w!(b"\r\n");
    Ok(len)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use naia_socket_shared::{PROTOCOL_ID_HEADER, PROTOCOL_ID_HEADER_VALUE_LEN};

    use super::{
        read_session_request, RtcUrlPaths, CORS_ALLOW_HEADERS, MAX_HEADER_BYTES,
        MAX_REQUEST_LINE_BYTES,
    };

    /// A syntactically valid fingerprint: the right width, all hex.
    const GOOD_ID: &str = "0123456789abcdef0123456789abcdef";
    const BAD_ID: &str = "fedcba9876543210fedcba9876543210";

    fn paths() -> RtcUrlPaths {
        RtcUrlPaths {
            post: "POST /rtc_session".to_string(),
            options: "OPTIONS /rtc_session".to_string(),
        }
    }

    fn addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000)
    }

    fn read(request: &[u8]) -> Option<super::SessionRequest> {
        smol::block_on(read_session_request(request, &paths(), &addr()))
    }

    #[test]
    fn well_formed_post_is_parsed() {
        let request = read(
            b"POST /rtc_session HTTP/1.1\r\nAuthorization: token\r\nContent-Length: 5\r\n\r\nhello",
        )
        .expect("well-formed request should parse");
        assert!(!request.is_options);
        assert_eq!(request.auth_string.as_deref(), Some("token"));
        assert_eq!(request.body, b"hello");
    }

    #[test]
    fn well_formed_options_is_parsed() {
        let request = read(b"OPTIONS /rtc_session HTTP/1.1\r\n\r\n")
            .expect("well-formed OPTIONS should parse");
        assert!(request.is_options);
    }

    /// The session listener runs pre-authentication, so a peer can send whatever
    /// it likes. A non-UTF-8 header line used to be `String::from_utf8(..).expect(..)`
    /// -- one unauthenticated packet was enough to panic the task and take the
    /// server process down.
    #[test]
    fn non_utf8_header_line_is_rejected_not_panicked_on() {
        let mut request = b"POST /rtc_session HTTP/1.1\r\n".to_vec();
        request.extend_from_slice(&[0xff, 0xfe, b'\r', b'\n']);
        request.extend_from_slice(b"\r\n");
        assert!(read(&request).is_none());
    }

    /// An unterminated header line must not be buffered without bound.
    #[test]
    fn over_long_header_line_is_rejected() {
        let mut request = b"POST /rtc_session HTTP/1.1\r\n".to_vec();
        request.extend(std::iter::repeat_n(b'a', MAX_REQUEST_LINE_BYTES + 1));
        assert!(read(&request).is_none());
    }

    /// Neither may an endless run of short, well-formed header lines.
    #[test]
    fn over_long_headers_are_rejected() {
        let mut request = b"POST /rtc_session HTTP/1.1\r\n".to_vec();
        while request.len() <= MAX_HEADER_BYTES {
            request.extend_from_slice(b"X: y\r\n");
        }
        request.extend_from_slice(b"\r\n");
        assert!(read(&request).is_none());
    }

    /// `Content-Length` is attacker-declared and sizes a `Vec`, so it needs a cap
    /// of its own -- the header cap above stops counting once headers end.
    #[test]
    fn over_large_content_length_is_rejected() {
        let request = b"POST /rtc_session HTTP/1.1\r\nContent-Length: 4294967296\r\n\r\n".to_vec();
        assert!(read(&request).is_none());
    }

    #[test]
    fn truncated_request_is_rejected() {
        assert!(read(b"POST /rtc_session HTTP/1.1\r\nContent-Length: 5\r\n\r\nhi").is_none());
    }

    // ---- protocol fingerprint ----------------------------------------------

    /// Builds a POST carrying `raw` verbatim as the fingerprint header value.
    fn read_with_fingerprint(raw: &str) -> Option<super::SessionRequest> {
        read(
            format!(
                "POST /rtc_session HTTP/1.1\r\nAuthorization: token\r\n{}: {}\r\nContent-Length: 5\r\n\r\nhello",
                PROTOCOL_ID_HEADER, raw
            )
            .as_bytes(),
        )
    }

    /// F13a: a well-formed fingerprint reaches the gate intact.
    #[test]
    fn a_well_formed_fingerprint_is_carried_through_the_parser() {
        let request = read_with_fingerprint(GOOD_ID).expect("should parse");
        assert_eq!(request.protocol_id.as_deref(), Some(GOOD_ID));
        // ...and it did not disturb the credential beside it.
        assert_eq!(request.auth_string.as_deref(), Some("token"));
    }

    /// F13b: absent, too short, too long and non-value-shaped all collapse to
    /// the same `None` *here*, in the parser, so that the gate downstream is
    /// physically unable to tell them apart -- there is no branch left for it
    /// to take. This is the property that keeps the refusal indistinguishable
    /// from outside; a parser that preserved "present but malformed" would
    /// hand the gate a distinction it could accidentally leak.
    #[test]
    fn absent_and_malformed_and_wrong_width_are_one_indistinguishable_case() {
        let absent = read(
            b"POST /rtc_session HTTP/1.1\r\nAuthorization: token\r\nContent-Length: 5\r\n\r\nhello",
        )
        .expect("should parse")
        .protocol_id;
        assert_eq!(absent, None);

        for malformed in [
            "",
            &GOOD_ID[..PROTOCOL_ID_HEADER_VALUE_LEN - 1],
            &format!("{}0", GOOD_ID),
            &format!("0x{}", &GOOD_ID[2..]),
            &" ".repeat(PROTOCOL_ID_HEADER_VALUE_LEN + 4),
        ] {
            assert_eq!(
                read_with_fingerprint(malformed)
                    .expect("the request itself is still well-formed")
                    .protocol_id,
                absent,
                "{:?} must be indistinguishable from an absent fingerprint",
                malformed
            );
        }
    }

    /// The header name is matched case-insensitively, as HTTP requires: a
    /// client that title-cases it must not be treated as having omitted it.
    #[test]
    fn the_fingerprint_header_name_is_matched_case_insensitively() {
        let request = read(
            format!(
                "POST /rtc_session HTTP/1.1\r\nX-Naia-Protocol-Id: {}\r\nContent-Length: 5\r\n\r\nhello",
                GOOD_ID
            )
            .as_bytes(),
        )
        .expect("should parse");
        assert_eq!(request.protocol_id.as_deref(), Some(GOOD_ID));
    }

    /// F12: the CORS preflight must advertise the fingerprint header, or a
    /// browser refuses to send the POST that carries it and every wasm client
    /// fails to connect -- with a CORS error, not a protocol mismatch, which
    /// is a considerably worse thing to debug.
    #[test]
    fn the_cors_preflight_advertises_the_fingerprint_header() {
        assert!(
            CORS_ALLOW_HEADERS
                .to_lowercase()
                .contains(PROTOCOL_ID_HEADER),
            "{:?} must name {:?}",
            CORS_ALLOW_HEADERS,
            PROTOCOL_ID_HEADER
        );
        // The pre-existing entries must survive alongside it.
        assert!(CORS_ALLOW_HEADERS.to_lowercase().contains("authorization"));
        assert!(CORS_ALLOW_HEADERS.to_lowercase().contains("content-length"));
    }

    /// The gate itself, on the extracted decision.
    ///
    /// The parser tests above prove that absent, truncated, padded and non-hex
    /// values all collapse to `None` before the gate sees them. This proves the
    /// other half: that `None` is refused rather than waved through. Both halves
    /// are needed -- a parser that normalises everything to `None` is worth
    /// nothing if `None` then passes.
    #[test]
    fn a_post_without_an_acceptable_fingerprint_is_refused() {
        // Positive control: the gate is not refusing everything.
        assert!(super::fingerprint_is_acceptable(
            Some(GOOD_ID),
            GOOD_ID,
            false
        ));

        for (label, value) in [
            ("absent", None),
            ("wrong value", Some(BAD_ID)),
            ("empty", Some("")),
        ] {
            assert!(
                !super::fingerprint_is_acceptable(value, GOOD_ID, false),
                "a POST with a {label} fingerprint must be refused",
            );
        }
    }

    /// The OPTIONS exemption is exactly that -- an exemption for the preflight,
    /// which by construction carries no custom headers. It must not leak into
    /// the POST that follows, which the test above pins.
    #[test]
    fn the_cors_preflight_is_exempt_but_only_the_preflight() {
        assert!(super::fingerprint_is_acceptable(None, GOOD_ID, true));
        assert!(!super::fingerprint_is_acceptable(None, GOOD_ID, false));
    }

    /// The mismatch branch is wired to the shared constant in the source, not
    /// just in the outcome: a serve that routed a fingerprint failure back to
    /// the generic 404 would still refuse, and behaviour alone could not tell
    /// the regression from the contract. Sliced to the `serve` body so no
    /// other function or this test itself can satisfy it.
    #[test]
    fn the_mismatch_branch_is_coupled_to_the_shared_constant_in_source() {
        const THIS_FILE: &str = include_str!("session.rs");

        let start = THIS_FILE.find("async fn serve(").expect("serve must exist");
        let body = &THIS_FILE[start..];
        let end = body
            .find("\nfn response_header_to_vec")
            .expect("serve must be closed");
        let body = &body[..end];

        assert!(
            body.contains("PROTOCOL_MISMATCH_STATUS"),
            "serve must answer a fingerprint mismatch from the shared constant",
        );
        assert!(
            body.contains("fingerprint_mismatch"),
            "serve must track the mismatch apart from malformed framing",
        );
        assert!(
            body.contains("RESPONSE_BAD"),
            "malformed framing must still get the generic answer",
        );
    }

    /// The mismatch answer is the shared reserved status with an empty body:
    /// distinct from the 401 an application rejection gets and from the 404
    /// malformed framing gets, carrying neither fingerprint.
    #[test]
    fn the_mismatch_answer_is_reserved_payload_free_and_distinct() {
        use naia_socket_shared::PROTOCOL_MISMATCH_STATUS;

        assert_ne!(PROTOCOL_MISMATCH_STATUS, 401);
        assert_ne!(PROTOCOL_MISMATCH_STATUS, 404);
        assert_ne!(PROTOCOL_MISMATCH_STATUS, 400);
        assert_ne!(PROTOCOL_MISMATCH_STATUS, 500);

        let response = http::Response::builder()
            .status(PROTOCOL_MISMATCH_STATUS)
            .header(http::header::CONTENT_LENGTH, 0)
            .body(Vec::<u8>::new())
            .expect("mismatch response must build");
        assert_eq!(response.status().as_u16(), PROTOCOL_MISMATCH_STATUS);
        assert!(response.body().is_empty());
    }
}
