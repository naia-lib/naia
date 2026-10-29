use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Error, Result};
use async_net::TcpStream;
use bytes::Bytes;
use futures_channel::oneshot;
use futures_util::{
    io::{AsyncReadExt, AsyncWriteExt},
    select, FutureExt,
};
use log::warn;
use smol::channel;
use tinyjson::JsonValue;

use crate::webrtc::{
    data_channel::internal::data_channel::DataChannel,
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
};

const MESSAGE_SIZE: usize = 1500;

/// Maximum size in bytes of a single outgoing message.
///
/// This is the client's SCTP single-DATA-chunk payload limit:
/// `INITIAL_MTU (1228) - COMMON_HEADER_SIZE (12) - DATA_CHUNK_HEADER_SIZE (16)`
/// (see `webrtc/crates/sctp/association`). Messages larger than this would be
/// fragmented across multiple SCTP packets, and the `webrtc-unreliable` server
/// this client speaks to drops fragmented SCTP packets by design (its
/// `client.rs`: "received fragmented SCTP packet, dropping") — so an oversize
/// send can never arrive. We therefore reject oversize messages at the send
/// boundary with a logged error rather than letting them vanish silently.
pub const MAX_MESSAGE_SIZE: usize = 1200;

/// A signaling session request that did not yield an identity token.
///
/// The `webrtc-unreliable` server this client speaks to answers a refused
/// connection with a 401 whose body may carry an application-defined reason.
/// Reporting the status code alone would throw that away, so both travel back
/// to the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionError {
    /// The HTTP status the server answered with. `500` when the response
    /// arrived but could not be parsed.
    pub status_code: u16,
    /// The response body, verbatim, and empty when there was none. This crate
    /// does not interpret it.
    pub body: String,
}

impl SessionError {
    fn new(status_code: u16, body: String) -> Self {
        Self { status_code, body }
    }
}

pub struct Socket {
    to_client_addr_sender: oneshot::Sender<SocketAddr>,
    to_server_receiver: channel::Receiver<Box<[u8]>>,
    to_server_disconnect_receiver: channel::Receiver<()>,
    to_client_sender: channel::Sender<Box<[u8]>>,
    to_client_id_sender: oneshot::Sender<Result<String, SessionError>>,
}

pub struct SocketIo {
    /// Resolves once, with the server's data-channel address, as soon as the
    /// server's ICE candidate has been parsed. Consumers that want a polled
    /// "found it yet?" view build one over this; this crate does not.
    pub to_client_addr_receiver: oneshot::Receiver<SocketAddr>,
    pub to_server_sender: channel::Sender<Box<[u8]>>,
    pub to_server_disconnect_sender: channel::Sender<()>,
    pub to_client_receiver: channel::Receiver<Box<[u8]>>,
    pub to_client_id_receiver: oneshot::Receiver<Result<String, SessionError>>,
}

impl Socket {
    pub fn new() -> (Self, SocketIo) {
        let (to_client_addr_sender, to_client_addr_receiver) = oneshot::channel();
        let (to_server_sender, to_server_receiver) = channel::unbounded();
        let (to_server_disconnect_sender, to_server_disconnect_receiver) = channel::bounded(1);
        let (to_client_sender, to_client_receiver) = channel::unbounded();
        let (to_client_id_sender, to_client_id_receiver) = oneshot::channel();

        (
            Self {
                to_client_addr_sender,
                to_server_receiver,
                to_server_disconnect_receiver,
                to_client_sender,
                to_client_id_sender,
            },
            SocketIo {
                to_client_addr_receiver,
                to_server_sender,
                to_server_disconnect_sender,
                to_client_receiver,
                to_client_id_receiver,
            },
        )
    }

    pub async fn connect(
        self,
        server_url: &str,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) {
        let Self {
            to_client_addr_sender,
            to_server_receiver,
            to_server_disconnect_receiver,
            to_client_sender,
            to_client_id_sender,
        } = self;

        // create a new RTCPeerConnection
        let peer_connection = RTCPeerConnection::new().await;

        let label = "data";
        let protocol = "";

        // create a datachannel with label 'data'
        let data_channel = peer_connection
            .create_data_channel(label, protocol)
            .await
            .expect("cannot create data channel");

        // datachannel on_error callback
        data_channel
            .on_error(Box::new(move |error| {
                println!("data channel error: {:?}", error);
                Box::pin(async {})
            }))
            .await;

        // datachannel on_open callback
        let peer_connection_ref = Arc::clone(&peer_connection);
        let data_channel_ref = Arc::clone(&data_channel);
        data_channel
            .on_open(Box::new(move || {
                let peer_connection_ref_2 = Arc::clone(&peer_connection_ref);
                let data_channel_ref_2 = Arc::clone(&data_channel_ref);
                Box::pin(async move {
                    let detached_data_channel = data_channel_ref_2
                        .detach()
                        .await
                        .expect("data channel detach got error");

                    // Handle reading from the data channel
                    let peer_connection_ref_3 = Arc::clone(&peer_connection_ref_2);
                    let peer_connection_ref_4 = Arc::clone(&peer_connection_ref_2);

                    let detached_data_channel_1 = Arc::clone(&detached_data_channel);
                    let detached_data_channel_2 = Arc::clone(&detached_data_channel);
                    smol::spawn(async move {
                        let _loop_result =
                            read_loop(detached_data_channel_1, to_client_sender).await;

                        // do nothing with result, just close thread
                        peer_connection_ref_3.internal.close().await;
                    })
                    .detach();

                    // Handle writing to the data channel
                    smol::spawn(async move {
                        let detached_data_channel_3 = Arc::clone(&detached_data_channel_2);
                        let _loop_result = write_loop(
                            detached_data_channel_3,
                            to_server_receiver,
                            to_server_disconnect_receiver,
                        )
                        .await;

                        // do nothing with result, just close thread
                        detached_data_channel_2.close().await;

                        peer_connection_ref_4.internal.close().await;
                    })
                    .detach();
                })
            }))
            .await;

        // create an offer to send to the server
        let offer = peer_connection
            .create_offer()
            .await
            .expect("cannot create offer");

        // sets the LocalDescription, and starts our UDP listeners
        peer_connection
            .set_local_description(offer)
            .await
            .expect("cannot set local description");

        // send a request to server to initiate connection (signaling, essentially)
        let sdp = peer_connection.local_description().await.unwrap().sdp;

        let mut extra_headers: Vec<(String, String)> = Vec::new();
        if let Some(auth_bytes) = auth_bytes_opt {
            extra_headers.push(("Authorization".to_string(), base64::encode(auth_bytes)));
        }
        if let Some(auth_headers) = auth_headers_opt {
            extra_headers.extend(auth_headers);
        }

        // wait to receive a response from server
        let (status_code, response_string) = loop {
            match http_post(server_url, &sdp, &extra_headers).await {
                Ok(resp) => {
                    break resp;
                }
                Err(err) => {
                    warn!("Could not send request, original error: {:?}", err);
                    smol::Timer::after(Duration::from_secs(1)).await;
                }
            };
        };

        if !(200..300).contains(&status_code) {
            to_client_id_sender
                .send(Err(SessionError::new(status_code, response_string)))
                .unwrap();
            return;
        }

        // parse session from server response
        let session_response_result = get_session_response(response_string.as_str());
        let session_response = match session_response_result {
            Ok(session_response) => session_response,
            Err(_err) => {
                // parsing error?
                to_client_id_sender
                    .send(Err(SessionError::new(500, response_string)))
                    .unwrap();
                return;
            }
        };

        // send the id token to the client
        // info!("Sending id token to client: {:?}", auth_header);
        if let Err(err) = to_client_id_sender.send(Ok(session_response.id_token)) {
            warn!("Could not send id token to client: {:?}. Did the IdentityReceiver returned from Socket::connect() de-allocate?", err);
            return;
        }

        // apply the server's response as the remote description
        let session_description =
            RTCSessionDescription::answer(session_response.answer.sdp).unwrap();

        peer_connection
            .set_remote_description(session_description)
            .await
            .expect("cannot set remote description");

        // Hand the caller the server's data address. Parsing stays here because
        // the candidate line is this crate's business; the caller gets a plain
        // SocketAddr.
        match candidate_to_addr(session_response.candidate.candidate.as_str()) {
            Some(addr) => {
                let _ = to_client_addr_sender.send(addr);
            }
            None => warn!(
                "no SocketAddr found in ICE candidate: {}",
                session_response.candidate.candidate
            ),
        }

        // add ice candidate to connection
        if let Err(error) = peer_connection
            .add_ice_candidate(session_response.candidate.candidate)
            .await
        {
            panic!("Error during add_ice_candidate: {:?}", error);
        }
    }
}

/// Minimal HTTP/1.1 POST used for the one-shot `webrtc-unreliable` signaling
/// exchange, replacing the reqwest dependency. Plain `http://` only — the
/// session endpoint this client speaks to is served over plain HTTP, and the
/// exchanged SDP is authenticated separately by certificate fingerprint.
/// Returns (status_code, body).
async fn http_post(
    server_url: &str,
    body: &str,
    extra_headers: &[(String, String)],
) -> Result<(u16, String)> {
    let rest = server_url
        .strip_prefix("http://")
        .ok_or_else(|| Error::msg("only plain http:// URLs are supported for signaling"))?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{}:80", host_port)
    };

    let mut stream = TcpStream::connect(&addr).await?;

    let mut request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n",
        path,
        host_port,
        body.len()
    );
    for (key, value) in extra_headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).await?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let raw = String::from_utf8_lossy(&raw).into_owned();

    let (head, response_body) = raw
        .split_once("\r\n\r\n")
        .ok_or_else(|| Error::msg("malformed HTTP response"))?;
    let status_code: u16 = head
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| Error::msg("malformed HTTP status line"))?
        .parse()?;

    let lower_head = head.to_ascii_lowercase();
    let response_body = if lower_head.contains("transfer-encoding: chunked") {
        dechunk(response_body)?
    } else {
        // We read to EOF, so anything the server pipelined after this response
        // is still in the buffer. Content-Length says where the body ends.
        match content_length(&lower_head) {
            Some(len) if len <= response_body.len() => response_body[..len].to_string(),
            _ => response_body.to_string(),
        }
    };

    Ok((status_code, response_body))
}

/// Reads the `Content-Length` header out of an already-lowercased header block.
fn content_length(lower_head: &str) -> Option<usize> {
    lower_head
        .split("\r\n")
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Decodes an HTTP/1.1 chunked transfer-encoded body.
fn dechunk(mut input: &str) -> Result<String> {
    let mut out = String::new();
    loop {
        let (size_line, rest) = input
            .split_once("\r\n")
            .ok_or_else(|| Error::msg("malformed chunked body"))?;
        let size = usize::from_str_radix(size_line.trim().split(';').next().unwrap_or(""), 16)
            .map_err(|_| Error::msg("malformed chunk size"))?;
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size + 2 {
            return Err(Error::msg("truncated chunked body"));
        }
        out.push_str(&rest[..size]);
        input = &rest[size + 2..];
    }
}

// read_loop shows how to read from the datachannel directly
async fn read_loop(
    data_channel: Arc<DataChannel>,
    to_client_sender: channel::Sender<Box<[u8]>>,
) -> Result<()> {
    let mut buffer = vec![0u8; MESSAGE_SIZE];
    loop {
        let message_length = match data_channel.read(&mut buffer).await {
            Ok(length) => length,
            Err(_err) => {
                //println!("Datachannel closed; Exit the read_loop: {}", err);
                return Ok(());
            }
        };

        if let Err(e) = to_client_sender.try_send(buffer[..message_length].into()) {
            return Err(Error::msg(format!("send failed: {e:?}")));
        }
    }
}

// write_loop shows how to write to the datachannel directly
async fn write_loop(
    data_channel: Arc<DataChannel>,
    to_server_receiver: channel::Receiver<Box<[u8]>>,
    to_server_disconnect_receiver: channel::Receiver<()>,
) -> Result<()> {
    loop {
        select! {
            _ = to_server_disconnect_receiver.recv().fuse() => {
                return Ok(());
            }
            result = to_server_receiver.recv().fuse() => {
                if let Ok(mut write_message) = result {
                    if write_message.len() > MAX_MESSAGE_SIZE {
                        log::error!(
                            "dropping outgoing message of {} bytes: exceeds MAX_MESSAGE_SIZE ({} bytes); \
                             larger messages would be fragmented at the SCTP layer and dropped by the server",
                            write_message.len(),
                            MAX_MESSAGE_SIZE
                        );
                        continue;
                    }
                    let taken_message = std::mem::take(&mut write_message);
                    let message_bytes = Bytes::from(taken_message);
                    if let Err(e) = data_channel.write(&message_bytes).await {
                        return Err(Error::new(e));
                    }
                } else {
                    return Ok(());
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct SessionAnswer {
    pub(crate) sdp: String,
}

pub(crate) struct SessionCandidate {
    pub(crate) candidate: String,
}

pub(crate) struct JsSessionResponse {
    pub(crate) id_token: String,
    pub(crate) answer: SessionAnswer,
    pub(crate) candidate: SessionCandidate,
}

fn get_session_response(input: &str) -> Result<JsSessionResponse, String> {
    // info!("{}", input);
    let Ok(json_obj): Result<JsonValue, _> = input.parse() else {
        return Err("Could not parse response JSON".to_string());
    };

    let sdp_opt: Option<&String> = json_obj["sdp"]["answer"]["sdp"].get();
    let sdp: String = sdp_opt.unwrap().clone();

    let candidate_opt: Option<&String> = json_obj["sdp"]["candidate"]["candidate"].get();
    let candidate: String = candidate_opt.unwrap().clone();

    let id_token_opt: Option<&String> = json_obj["id"].get();
    let id_token: String = id_token_opt.unwrap().clone();

    Ok(JsSessionResponse {
        id_token,
        answer: SessionAnswer { sdp },
        candidate: SessionCandidate { candidate },
    })
}

/// Extracts the server's address from an ICE candidate line.
///
/// The line looks like
/// "candidate:<foundation> <component> udp <priority> <ip> <port> typ host ...";
/// we take the first adjacent (ip, port) token pair.
fn candidate_to_addr(candidate_str: &str) -> Option<SocketAddr> {
    let tokens: Vec<&str> = candidate_str.split_whitespace().collect();
    for w in tokens.windows(2) {
        if let Ok(ip_addr) = w[0].parse::<IpAddr>() {
            if let Ok(port) = w[1].parse::<u16>() {
                return Some(SocketAddr::new(ip_addr, port));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_address_out_of_a_host_candidate() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 127.0.0.1 14192 typ host"),
            Some("127.0.0.1:14192".parse().unwrap())
        );
    }

    #[test]
    fn reads_an_ipv6_address() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 ::1 14192 typ host"),
            Some("[::1]:14192".parse().unwrap())
        );
    }

    #[test]
    fn parses_an_srflx_candidate_with_a_raddr_tail() {
        // Server-reflexive candidates carry a longer tail than host ones. The
        // raddr address in it is followed by the literal "rport", so it never
        // forms a second (ip, port) pair for the scan to trip over.
        assert_eq!(
            candidate_to_addr("candidate:842163049 1 udp 1686052607 10.0.0.5 51234 typ srflx raddr 0.0.0.0 rport 0"),
            Some("10.0.0.5:51234".parse().unwrap())
        );
    }

    #[test]
    fn returns_none_when_there_is_no_address() {
        // This is the branch that used to panic. It now warns and leaves the
        // address channel unresolved, so the None has to actually come back.
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 typ host"),
            None
        );
        assert_eq!(candidate_to_addr(""), None);
        assert_eq!(candidate_to_addr("not a candidate line at all"), None);
    }

    #[test]
    fn returns_none_when_the_port_is_not_a_port() {
        // An address with no valid port beside it is not a usable pair.
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 127.0.0.1 99999 typ host"),
            None
        );
    }
}
