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
use once_cell::sync::OnceCell;
use smol::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines},
    lock::Mutex,
    stream::StreamExt,
    Async,
};
use webrtc_unreliable::SessionEndpoint;

use naia_socket_shared::{IdentityToken, SocketConfig};

use crate::{executor, server_addrs::ServerAddrs, NaiaServerSocketError};

static RTC_URL_POST_PATH: OnceCell<String> = OnceCell::new();
static RTC_URL_OPTIONS_PATH: OnceCell<String> = OnceCell::new();

pub fn start_session_server(
    server_addrs: ServerAddrs,
    config: SocketConfig,
    session_endpoint: SessionEndpoint,
    from_client_auth_sender: Option<
        smol::channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    >,
    to_session_all_auth_receiver: Option<
        smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
    >,
) {
    RTC_URL_POST_PATH
        .set(format!("POST /{}", config.rtc_endpoint_path))
        .expect("unable to set the URL Path");
    RTC_URL_OPTIONS_PATH
        .set(format!("OPTIONS /{}", config.rtc_endpoint_path))
        .expect("unable to set the URL Path");
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
    from_client_auth_sender: Option<
        smol::channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    >,
    to_session_all_auth_receiver: Option<
        smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
    >,
) {
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
        // Spawn a background task serving this connection.
        executor::spawn(async move {
            serve(
                session_endpoint_clone,
                Arc::new(response_stream),
                from_client_auth_sender,
                to_session_single_auth_receiver,
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
    map: Arc<
        Mutex<
            HashMap<
                SocketAddr,
                (
                    Option<futures_channel::oneshot::Sender<Option<IdentityToken>>>,
                    Option<Option<IdentityToken>>,
                ),
            >,
        >,
    >,
    to_session_all_auth_receiver: smol::channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
) {
    loop {
        let Ok((addr, answer)) = to_session_all_auth_receiver.recv().await else {
            warn!("Unable to receive auth from session");
            continue;
        };

        // info!("received auth answer from app, for addr: {}, answer: {}", addr, answer);

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
    map: Arc<
        Mutex<
            HashMap<
                SocketAddr,
                (
                    Option<futures_channel::oneshot::Sender<Option<IdentityToken>>>,
                    Option<Option<IdentityToken>>,
                ),
            >,
        >,
    >,
    sender_receiver: smol::channel::Receiver<(
        SocketAddr,
        futures_channel::oneshot::Sender<Option<IdentityToken>>,
    )>,
) {
    loop {
        let Ok((addr, sender)) = sender_receiver.recv().await else {
            warn!("Unable to receive auth sender from session");
            continue;
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

/// Reads a request from the client and sends it a response.
async fn serve(
    mut session_endpoint: SessionEndpoint,
    mut stream: Arc<Async<TcpStream>>,
    from_client_auth_sender: Option<
        smol::channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
    >,
    to_session_single_auth_receiver: Option<
        futures_channel::oneshot::Receiver<Option<IdentityToken>>,
    >,
) {
    let remote_addr = stream
        .get_ref()
        .peer_addr()
        .expect("stream does not have a local address");

    info!("Incoming WebRTC session request from {}", remote_addr);

    let mut success: bool = false;
    let mut headers_been_read: bool = false;
    let mut content_length: Option<usize> = None;
    let mut auth_string: Option<String> = None;
    let mut rtc_url_matched = false;
    let mut is_options: bool = false;
    let mut body: Vec<u8> = Vec::new();
    let mut identity_token_opt = None;

    let buf_reader = BufReader::new(stream.clone());
    let mut bytes = buf_reader.bytes();
    {
        let mut line: Vec<u8> = Vec::new();
        while let Some(byte) = bytes.next().await {
            let byte = byte.expect("unable to read a byte from incoming stream");

            if headers_been_read {
                if let Some(content_length) = content_length {
                    body.push(byte);

                    if body.len() >= content_length {
                        // info!("read body finished");
                        success = true;
                        break;
                    }
                } else {
                    info!("request was missing Content-Length header");
                    break;
                }
            }

            if byte == b'\r' {
                continue;
            } else if byte == b'\n' {
                let mut str = String::from_utf8(line.clone())
                    .expect("unable to parse string from UTF-8 bytes");
                line.clear();

                if rtc_url_matched {
                    if str.to_lowercase().starts_with("content-length: ") {
                        let (_, last) = str.split_at(16);
                        str = last.to_string();
                        content_length = str.parse::<usize>().ok();
                        // info!("read content length header: {:?}", content_length);
                    } else if str.to_lowercase().starts_with("authorization: ") {
                        let (_, last) = str.split_at(15);
                        auth_string = Some(last.to_string());
                        // info!("read authorization header: {:?}", auth_string);
                    } else if str.is_empty() {
                        // info!("read headers finished");
                        headers_been_read = true;

                        if is_options {
                            success = true;
                            break;
                        }
                    } else {
                        // info!("read leftover line 1: {}", str);
                    }
                } else if str.starts_with(
                    RTC_URL_POST_PATH
                        .get()
                        .expect("unable to retrieve URL path, was it not configured?"),
                ) {
                    // info!("starting to match to RTC URL");
                    rtc_url_matched = true;
                } else if str.starts_with(
                    RTC_URL_OPTIONS_PATH
                        .get()
                        .expect("unable to retrieve URL path, was it not configured?"),
                ) {
                    // info!("matched OPTIONS request for RTC URL");
                    rtc_url_matched = true;
                    is_options = true;
                } else {
                    // info!("read leftover line 2: {}", str);
                }
            } else {
                line.push(byte);
            }
        }

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

            stream
                .write_all(&out)
                .await
                .expect("found an error while writing to a stream");
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
                                if let Ok(Some(identity_token)) = to_session_auth_receiver.await {
                                    // info!("Server app accepted auth with identity token: {}", identity_token);
                                    identity_token_opt = Some(identity_token);
                                    success = true;
                                }
                            }
                        }
                        Err(_) => {
                            warn!("Invalid WebRTC session request from {}. Error: unable to decode auth string", remote_addr);
                        }
                    }
                } else {
                    warn!("Invalid WebRTC session request from {}. Error: missing auth string", remote_addr);
                }
            } else {
                warn!("Invalid WebRTC session request from {}. Error: missing auth sender", remote_addr);
            }
        }

        // read body and init session
        if success && !is_options {
            success = false;

            // info!("reading identity token");

            let identity_token = identity_token_opt.take().unwrap();

            // info!("identity token: {:?}", identity_token);

            let mut lines = body.lines();
            let buf = RequestBuffer::new(&mut lines);

            match session_endpoint.http_session_request(buf).await {
                Ok(resp) => {

                    info!("Successful WebRTC session request");

                    success = true;

                    let (_head, body) = resp.into_parts();

                    let body = format!(
                        "{{\
                        \"sdp\":{body},\
                        \"id\":\"{identity_token}\"\
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

                    stream
                        .write_all(&out)
                        .await
                        .expect("found an error while writing to a stream");
                }
                Err(err) => {
                    warn!(
                        "Invalid WebRTC session request from {}. Error: {}",
                        remote_addr, err
                    );
                }
            }
        }
    }

    // info!("Closing WebRTC session request from {}", remote_addr);

    if !success {
        stream.write_all(RESPONSE_BAD).await.expect("found");
    }

    stream.flush().await.expect("unable to flush the stream");
    stream.close().await.expect("unable to close the stream");
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
            unsafe {
                let mut_ref = Pin::new_unchecked(&mut self.buffer);
                match Stream::poll_next(mut_ref, cx) {
                    Poll::Ready(Some(item)) => {
                        self.add_newline = true;
                        Poll::Ready(Some(item))
                    }
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => {
                        // TODO: This could be catastrophic.. I don't understand futures very
                        // well!
                        Poll::Ready(None)
                    }
                }
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
