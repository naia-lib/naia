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

use naia_socket_shared::{IdentityToken, SocketConfig};

use crate::{executor, server_addrs::ServerAddrs, NaiaServerSocketError};

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

type ClientAuthSender =
    smol::channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;

type AuthMuxMap = Arc<
    Mutex<
        HashMap<
            SocketAddr,
            (
                Option<futures_channel::oneshot::Sender<Option<IdentityToken>>>,
                Option<Option<IdentityToken>>,
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
    to_session_all_auth_receiver: Option<
        smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
    >,
) {
    executor::spawn(async move {
        listen(
            server_addrs,
            config,
            session_endpoint.clone(),
            from_client_auth_sender,
            to_session_all_auth_receiver,
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
    to_session_all_auth_receiver: Option<
        smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
    >,
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
        // Spawn a background task serving this connection.
        executor::spawn(async move {
            serve(
                session_endpoint_clone,
                Arc::new(response_stream),
                from_client_auth_sender,
                to_session_single_auth_receiver,
                rtc_url_paths,
            )
            .await;
        })
        .detach();
    }
}

async fn setup_auth_mux(
    to_session_all_auth_receiver: smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
) -> smol::channel::Sender<(
    SocketAddr,
    futures_channel::oneshot::Sender<Option<IdentityToken>>,
)> {
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
    to_session_all_auth_receiver: smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
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
        futures_channel::oneshot::Sender<Option<IdentityToken>>,
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
                } else if str.is_empty() {
                    headers_been_read = true;

                    if is_options {
                        return Some(SessionRequest {
                            is_options,
                            auth_string,
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

/// Reads a request from the client and sends it a response.
async fn serve(
    mut session_endpoint: SessionEndpoint,
    mut stream: Arc<Async<TcpStream>>,
    from_client_auth_sender: Option<ClientAuthSender>,
    to_session_single_auth_receiver: Option<
        futures_channel::oneshot::Receiver<Option<IdentityToken>>,
    >,
    rtc_url_paths: RtcUrlPaths,
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
    let (mut success, is_options, auth_string, body) = match request {
        Some(request) => (true, request.is_options, request.auth_string, request.body),
        None => (false, false, None, Vec::new()),
    };
    let mut identity_token_opt = None;

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
            resp.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Authorization, Content-Length"),
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
                                if let Ok(identity_token_inner_opt) = to_session_auth_receiver.await
                                {
                                    if let Some(identity_token) = identity_token_inner_opt {
                                        // info!("Server app accepted auth with identity token: {}", identity_token);
                                        identity_token_opt = Some(identity_token);
                                        success = true;
                                    } else {
                                        // warn!("Server app rejected auth");
                                        identity_token_opt = None;
                                        success = true;
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
                let response = Response::builder()
                    .status(401)
                    .body("".to_string())
                    .expect("could not build 401 response");

                let mut out = response_header_to_vec(&response);
                out.extend_from_slice(response.body().as_bytes());

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
    if !success && stream.write_all(RESPONSE_BAD).await.is_err() {
        warn!("Error writing 404 response to {}", remote_addr);
        return;
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

    use super::{read_session_request, RtcUrlPaths, MAX_HEADER_BYTES, MAX_REQUEST_LINE_BYTES};

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
}
