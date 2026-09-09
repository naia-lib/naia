use parking_lot::Mutex;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use log::warn;

use naia_shared::{
    http_utils, IdentityToken, LinkConditionerConfig, ProtocolId, PROTOCOL_ID_HEADER,
    PROTOCOL_MISMATCH_STATUS,
};

use super::{
    AuthReceiver as TransportAuthReceiver, AuthSender as TransportAuthSender,
    ConditionedPacketReceiver, ListenResult, PacketReceiver, PacketSender as TransportSender,
    RecvError, SendError, Socket as TransportSocket,
};

/// Native UDP server socket.
///
/// # Security
///
/// **All traffic is unencrypted plaintext.** This transport is suitable for
/// local development and trusted private networks only. For internet-facing
/// deployments, use a transport with built-in TLS (e.g. `transport_quic`).
/// Credentials sent via `AuthEvent` are visible on the wire.
pub struct Socket {
    data_socket: Arc<Mutex<UdpSocket>>,
    /// Bound at construction so an address conflict surfaces there rather than
    /// at `listen()`, but not yet wrapped in an [`AuthIo`]: the `AuthIo` cannot
    /// exist until it knows the fingerprint it must compare against, which
    /// only arrives with `listen()`. Keeping the two apart is what makes a
    /// fingerprint-less auth path unrepresentable rather than merely
    /// discouraged.
    auth_socket: TcpListener,
    public_udp_url: String,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    /// Create a new plaintext UDP server socket.
    ///
    /// **Not suitable for untrusted networks** — see the type-level security
    /// note above.
    pub fn new(server_addrs: &ServerAddrs, config: Option<LinkConditionerConfig>) -> Self {
        let auth_socket = TcpListener::bind(server_addrs.auth_listen_addr).unwrap();
        auth_socket
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        let data_socket = Arc::new(Mutex::new(
            UdpSocket::bind(server_addrs.udp_listen_addr).unwrap(),
        ));
        data_socket
            .as_ref()
            .lock()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        Self {
            data_socket,
            auth_socket,
            public_udp_url: server_addrs.public_udp_url.clone(),
            config,
        }
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn listen(self: Box<Self>, expected_protocol_id: ProtocolId) -> ListenResult {
        let auth_io = Arc::new(Mutex::new(AuthIo::new(
            &self.public_udp_url,
            self.auth_socket,
            expected_protocol_id,
        )));
        let auth_sender = AuthSender::new(auth_io.clone());
        let auth_receiver = AuthReceiver::new(auth_io);
        let packet_sender = UdpPacketSender::new(self.data_socket.clone());
        let packet_receiver = UdpPacketReceiver::new(self.data_socket.clone());

        let packet_receiver: Box<dyn PacketReceiver> = {
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(
                    Box::new(packet_receiver),
                    config,
                ))
            } else {
                Box::new(packet_receiver)
            }
        };

        (
            Box::new(auth_sender),
            Box::new(auth_receiver),
            Box::new(packet_sender),
            packet_receiver,
        )
    }
}

// Packet Sender

#[derive(Clone)]
struct UdpPacketSender {
    socket: Arc<Mutex<UdpSocket>>,
}

impl UdpPacketSender {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        Self { socket }
    }
}

impl TransportSender for UdpPacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, socket_addr: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        if self
            .socket
            .as_ref()
            .lock()
            .send_to(payload, *socket_addr)
            .is_err()
        {
            return Err(SendError);
        }
        Ok(())
    }
}

// Packet Receiver
#[derive(Clone)]
pub(crate) struct UdpPacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    buffer: [u8; 1472],
}

impl UdpPacketReceiver {
    pub fn new(socket: Arc<Mutex<UdpSocket>>) -> Self {
        Self {
            socket,
            buffer: [0; 1472],
        }
    }
}

impl PacketReceiver for UdpPacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match self.socket.as_ref().lock().recv_from(&mut self.buffer) {
            Ok((recv_len, address)) => Ok(Some((address, &self.buffer[..recv_len]))),
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => Ok(None),
                    _ => Err(RecvError),
                }
            }
        }
    }
}

// AuthIo
/// Cap on a single inbound auth request, mirroring the ones the WebRTC session
/// listener applies. A peer can hold a connection open and dribble an
/// unterminated header line forever, so the header bytes are counted and the
/// attempt abandoned once it stops looking like a real auth request. The value
/// is far above any legitimate one.
const MAX_AUTH_HEADER_BYTES: usize = 16 * 1024;

/// How many half-read auth requests may be outstanding at once. Each holds an
/// accepted socket and its bytes so far, both of which a peer can create for
/// free, so the set needs a ceiling of its own. A policy bound, not a derived
/// one -- nothing in the protocol implies a number here.
const MAX_PENDING_AUTH_READS: usize = 256;

/// How long a peer has to finish sending its request once connected. Without
/// this it could sit just under `MAX_AUTH_HEADER_BYTES` indefinitely.
const PENDING_AUTH_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// An accepted connection whose request has not arrived in full yet.
struct PendingAuthRead {
    stream: TcpStream,
    bytes: Vec<u8>,
    started_at: Instant,
}

pub(crate) struct AuthIo {
    public_udp_addr: SocketAddr,
    socket: TcpListener,
    buffer: [u8; 1472],
    outgoing_streams: HashMap<SocketAddr, TcpStream>,
    /// Connections accepted but not yet read to the end of their headers. A
    /// single `read` is not guaranteed to deliver a whole request, and a stream
    /// the listener accepts is blocking even though the listener itself is not,
    /// so reading one inline let any peer stall the server's tick by connecting
    /// and then sending nothing.
    pending_reads: HashMap<SocketAddr, PendingAuthRead>,
    /// The fingerprint every incoming auth request must carry. Not an
    /// `Option`: there is no state in which this transport accepts auth
    /// without something to compare against.
    expected_protocol_id: ProtocolId,
}

impl AuthIo {
    pub fn new(
        public_udp_url: &str,
        socket: TcpListener,
        expected_protocol_id: ProtocolId,
    ) -> Self {
        let public_udp_addr = url_str_to_addr(public_udp_url);

        Self {
            public_udp_addr,
            socket,
            buffer: [0; 1472],
            outgoing_streams: HashMap::new(),
            pending_reads: HashMap::new(),
            expected_protocol_id,
        }
    }

    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        self.accept_new_streams();
        self.drop_timed_out_reads();

        let Some((addr, auth_bytes)) = self.poll_pending_reads() else {
            return Ok(None);
        };

        if auth_bytes.len() > self.buffer.len() {
            warn!("Over-large auth payload in auth request from {}", addr);
            self.outgoing_streams.remove(&addr);
            return Ok(None);
        }
        self.buffer[0..auth_bytes.len()].copy_from_slice(&auth_bytes);
        Ok(Some((addr, &self.buffer[..auth_bytes.len()])))
    }

    /// Drains the listener's accept queue, parking each new connection until
    /// its request has actually arrived.
    fn accept_new_streams(&mut self) {
        loop {
            let Ok((stream, addr)) = self.socket.accept() else {
                return;
            };

            if self.outgoing_streams.contains_key(&addr) || self.pending_reads.contains_key(&addr) {
                // already have a stream for this address
                // TODO: handle this case?
                continue;
            }

            if self.pending_reads.len() >= MAX_PENDING_AUTH_READS {
                warn!(
                    "pending auth read backlog full ({}); dropping connection from {}",
                    MAX_PENDING_AUTH_READS, addr
                );
                continue;
            }

            // The listener is non-blocking, but a stream it accepts is not, so
            // this has to be set explicitly -- otherwise the reads below would
            // block the whole server on a peer that has sent nothing.
            if stream.set_nonblocking(true).is_err() {
                continue;
            }

            self.pending_reads.insert(
                addr,
                PendingAuthRead {
                    stream,
                    bytes: Vec::new(),
                    started_at: Instant::now(),
                },
            );
        }
    }

    fn drop_timed_out_reads(&mut self) {
        self.pending_reads
            .retain(|_, pending| pending.started_at.elapsed() < PENDING_AUTH_READ_TIMEOUT);
    }

    /// Advances every half-read request, returning the auth payload of the
    /// first one that turns out to be a complete auth request.
    ///
    /// A connection whose request completes but is not an auth request -- no
    /// `Authorization` header, or a header that will not base64-decode -- is
    /// dropped rather than reported, which closes it. The stream is retained in
    /// `outgoing_streams` only for a request the application will be asked
    /// about, because only `accept`/`reject` ever remove it again.
    fn poll_pending_reads(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        let addrs: Vec<SocketAddr> = self.pending_reads.keys().copied().collect();

        for addr in addrs {
            let Some(pending) = self.pending_reads.get_mut(&addr) else {
                continue;
            };

            match read_until_headers_complete(pending) {
                HeaderReadState::Pending => continue,
                HeaderReadState::Failed => {
                    self.pending_reads.remove(&addr);
                    continue;
                }
                HeaderReadState::Complete => {}
            }

            let pending = self
                .pending_reads
                .remove(&addr)
                .expect("just read from this entry");

            let Some(auth_bytes) = decode_auth_request(&pending.bytes, &self.expected_protocol_id)
            else {
                // A fingerprint mismatch gets a prompt, payload-free,
                // reserved-status answer so the client fails fast with exactly
                // one `RejectEvent(ProtocolMismatch)` instead of hanging on a
                // dropped connection. Malformed framing is still dropped
                // silently and stays a generic transport error. Either way the
                // request never reaches the application: no user record, no
                // auth event, no token, and the credential is never decoded.
                if fingerprint_mismatch(&pending.bytes, &self.expected_protocol_id) {
                    let mut stream = pending.stream;
                    let _ = stream.set_nonblocking(false);
                    if let Ok(response) = http::Response::builder()
                        .status(PROTOCOL_MISMATCH_STATUS)
                        .body(Vec::new())
                    {
                        let response_bytes = http_utils::response_to_bytes(response);
                        let _ = stream.write_all(&response_bytes);
                        let _ = stream.flush();
                    }
                }
                continue;
            };

            // Put the stream back into blocking mode before handing it over:
            // `accept`/`reject` write the reply with `write_all`, which would
            // otherwise be able to fail with `WouldBlock` on a full send
            // buffer and lose an answer the application already gave.
            let stream = pending.stream;
            if stream.set_nonblocking(false).is_err() {
                continue;
            }

            self.outgoing_streams.insert(addr, stream);
            return Some((addr, auth_bytes));
        }

        None
    }

    /// Sends an accept packet from the Client Socket
    fn accept(
        &mut self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(address) {
            let response_body = format!(
                "{}\r\n{}",
                identity_token.to_signaling_string(),
                self.public_udp_addr
            );
            let response_body_bytes = response_body.into_bytes();

            let response = http::Response::builder()
                .status(200)
                .body(response_body_bytes)
                .map_err(|_| SendError)?;
            let response_bytes = http_utils::response_to_bytes(response);
            stream.write_all(&response_bytes).map_err(|_| SendError)?;
            stream.flush().map_err(|_| SendError)?;

            return Ok(());
        }
        Err(SendError)
    }

    /// Sends a rejection packet from the Client Socket
    ///
    /// `payload` is an optional serialized message explaining the rejection. It
    /// is base64-encoded into the response body so that every transport can
    /// carry it the same way, including the ones whose bodies are text.
    fn reject(&mut self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(address) {
            let body = match payload {
                Some(bytes) => base64::encode(bytes).into_bytes(),
                None => Vec::new(),
            };
            let response = http::Response::builder()
                .status(401)
                .body(body)
                .map_err(|_| SendError)?;
            let response_bytes = http_utils::response_to_bytes(response);
            stream.write_all(&response_bytes).map_err(|_| SendError)?;
            stream.flush().map_err(|_| SendError)?;

            return Ok(());
        }
        Err(SendError)
    }
}

enum HeaderReadState {
    /// The request headers have arrived in full.
    Complete,
    /// Nothing more is available yet; try again next tick.
    Pending,
    /// The peer hung up, errored, or overran the header cap.
    Failed,
}

/// Reads whatever is currently available on `pending`'s stream, stopping at the
/// blank line that ends the HTTP headers.
///
/// The body is deliberately not read: naia's auth request carries everything it
/// needs in the `Authorization` header, so waiting for a body would only give a
/// peer one more thing to stall on.
fn read_until_headers_complete(pending: &mut PendingAuthRead) -> HeaderReadState {
    let mut chunk = [0u8; 1024];

    loop {
        if headers_end(&pending.bytes).is_some() {
            return HeaderReadState::Complete;
        }
        if pending.bytes.len() > MAX_AUTH_HEADER_BYTES {
            warn!(
                "Over-long headers in auth request from {:?}",
                pending.stream.peer_addr().ok()
            );
            return HeaderReadState::Failed;
        }

        match pending.stream.read(&mut chunk) {
            // The peer closed the connection without finishing its request.
            Ok(0) => return HeaderReadState::Failed,
            Ok(len) => pending.bytes.extend_from_slice(&chunk[..len]),
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => return HeaderReadState::Pending,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return HeaderReadState::Failed,
        }
    }
}

/// Index just past the blank line terminating the headers, if it has arrived.
fn headers_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| start + 4)
}

/// Pulls the decoded `Authorization` payload out of a complete request, or
/// `None` if the request must not be answered.
///
/// # Fingerprint gate
///
/// The protocol fingerprint is checked **first**, before the credential is
/// even looked at, and every way of failing it — header absent, not valid
/// UTF-8, not 32 hex digits, or 32 hex digits naming a different protocol —
/// leaves through the same `None`. That indistinguishability is the point: a
/// peer learns only "refused", never which of those it was, and never the
/// expected value. The fingerprint is public compatibility metadata that
/// anybody with a released client can compute, so it is not a secret worth
/// protecting — but echoing it would turn a refusal into an oracle that hands
/// out the one value needed to get past this check, and nothing is gained by
/// that.
///
/// Failing here means the caller drops the connection without the request ever
/// reaching the server application: no user record, no auth event, no token,
/// and the credential is never base64-decoded, so it cannot be consumed.
fn decode_auth_request(bytes: &[u8], expected_protocol_id: &ProtocolId) -> Option<Vec<u8>> {
    let request = http_utils::bytes_to_request(bytes);

    let protocol_id = request
        .headers()
        .get(PROTOCOL_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(ProtocolId::from_hex);
    if protocol_id.as_ref() != Some(expected_protocol_id) {
        return None;
    }

    let auth_header = request.headers().get("Authorization")?;
    let auth_str = auth_header.to_str().ok()?;
    base64::decode(auth_str).ok()
}

/// Reports whether a complete request fails the fingerprint check alone.
///
/// Used to tell a protocol mismatch (prompt reserved-status answer) apart
/// from malformed framing (silent drop, generic transport error) without
/// decoding the credential: absent, non-UTF-8, wrong-width, and wrong-value
/// fingerprints all report `true` through this one condition, and the caller
/// answers them with identical bytes carrying neither fingerprint.
fn fingerprint_mismatch(bytes: &[u8], expected_protocol_id: &ProtocolId) -> bool {
    let request = http_utils::bytes_to_request(bytes);
    let protocol_id = request
        .headers()
        .get(PROTOCOL_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(ProtocolId::from_hex);
    protocol_id.as_ref() != Some(expected_protocol_id)
}

// AuthSender
#[derive(Clone)]
pub(crate) struct AuthSender {
    auth_io: Arc<Mutex<AuthIo>>,
}

impl AuthSender {
    pub fn new(auth_io: Arc<Mutex<AuthIo>>) -> Self {
        Self { auth_io }
    }
}

impl TransportAuthSender for AuthSender {
    /// Sends an accept packet from the Client Socket
    fn accept(
        &self,
        address: &SocketAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        self.auth_io.lock().accept(address, identity_token)
    }

    /// Sends a rejection packet from the Client Socket
    fn reject(&self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError> {
        self.auth_io.lock().reject(address, payload)
    }
}

// AuthReceiver
#[derive(Clone)]
pub(crate) struct AuthReceiver {
    auth_io: Arc<Mutex<AuthIo>>,
    buffer: Box<[u8]>,
}

impl AuthReceiver {
    pub fn new(auth_io: Arc<Mutex<AuthIo>>) -> Self {
        Self {
            auth_io,
            buffer: Box::new([0; 1472]),
        }
    }
}

impl TransportAuthReceiver for AuthReceiver {
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        let mut guard = self.auth_io.lock();
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

/// List of addresses needed to start listening on a ServerSocket
#[derive(Clone)]
pub struct ServerAddrs {
    /// IP Address to listen on for incoming auth requests
    pub auth_listen_addr: SocketAddr,
    /// IP Address to listen on for UDP data transmission
    pub udp_listen_addr: SocketAddr,
    /// The public IP address to advertise for UDP data transmission
    pub public_udp_url: String,
}

impl ServerAddrs {
    /// Create a new ServerAddrs instance which will be used to start
    /// listening on a ServerSocket
    pub fn new(
        auth_listen_addr: SocketAddr,
        udp_listen_addr: SocketAddr,
        public_udp_url: &str,
    ) -> Self {
        Self {
            auth_listen_addr,
            udp_listen_addr,
            public_udp_url: public_udp_url.to_string(),
        }
    }
}

impl Default for ServerAddrs {
    fn default() -> Self {
        Self::new(
            "127.0.0.1:14191"
                .parse()
                .expect("could not parse HTTP address/port"),
            "127.0.0.1:14192"
                .parse()
                .expect("could not parse UDP data address/port"),
            "http://127.0.0.1:14192",
        )
    }
}

use url::Url;

fn url_str_to_addr(url_str: &str) -> SocketAddr {
    let url = Url::parse(url_str)
        .unwrap_or_else(|e| panic!("server_url_str is not a valid URL (got: {url_str:?}): {e}"));
    if let Some(path_segments) = url.path_segments() {
        let path_segment_count = path_segments.count();
        if path_segment_count > 1 {
            panic!("server_url_str must not include a path (got: {url_str:?})");
        }
    }
    if url.query().is_some() {
        panic!("server_url_str must not include a query string (got: {url_str:?})");
    }
    if url.fragment().is_some() {
        panic!("server_url_str must not include a fragment (got: {url_str:?})");
    }

    url_to_addr(&url)
}

fn url_to_addr(url: &Url) -> SocketAddr {
    const SOCKET_PARSE_FAIL_STR: &str = "could not get SocketAddr from input URL";

    match url.socket_addrs(|| match url.scheme() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }) {
        Ok(addr_list) => {
            if addr_list.is_empty() {
                panic!("{SOCKET_PARSE_FAIL_STR}: {url}");
            }

            return *addr_list.first().expect(SOCKET_PARSE_FAIL_STR);
        }
        Err(err) => {
            panic!("URL -> SocketAddr parse fails for {url}: {err:?}");
        }
    }
}

#[cfg(test)]
mod auth_io_stream_tests {
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::Duration,
    };

    use naia_shared::{ProtocolId, PROTOCOL_ID_HEADER, PROTOCOL_MISMATCH_STATUS};

    use super::{decode_auth_request, fingerprint_mismatch, AuthIo, MAX_PENDING_AUTH_READS};

    /// The fingerprint the test server expects. Arbitrary, but fixed: what
    /// matters is that a peer presenting anything else is refused.
    fn expected_id() -> ProtocolId {
        ProtocolId::new(0xdead_beef)
    }

    /// The header line a well-behaved client sends, terminator included.
    fn fingerprint_line() -> String {
        format!("{}: {}\r\n", PROTOCOL_ID_HEADER, expected_id().to_hex())
    }

    fn auth_io() -> (AuthIo, u16) {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        socket.set_nonblocking(true).unwrap();
        (
            AuthIo::new("udp://127.0.0.1:14191", socket, expected_id()),
            port,
        )
    }

    /// Drives `receive` until it stops reporting `WouldBlock`-style emptiness,
    /// or gives up. Returns whether a request was surfaced to the application.
    fn drain_one(auth_io: &mut AuthIo) -> bool {
        for _ in 0..200 {
            match auth_io.receive() {
                Ok(Some(_)) => return true,
                Ok(None) => {}
                Err(_) => return false,
            }
            thread::sleep(Duration::from_millis(1));
        }
        false
    }

    fn send_request(port: u16, request: &str) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();
    }

    /// A request with no `Authorization` header is never handed to the
    /// application, so nothing ever calls `accept`/`reject` for it -- and only
    /// those remove a stream from `outgoing_streams`. Retaining it on accept
    /// leaked one live `TcpStream` per connection, to an unauthenticated peer.
    #[test]
    fn a_request_without_an_auth_header_does_not_retain_its_stream() {
        let (mut auth_io, port) = auth_io();

        for _ in 0..16 {
            send_request(
                port,
                &format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{}\r\n",
                    fingerprint_line()
                ),
            );
            drain_one(&mut auth_io);
        }

        assert!(
            auth_io.outgoing_streams.is_empty(),
            "an unauthenticated peer must not be able to accumulate open streams",
        );
    }

    /// A malformed `Authorization` value is likewise never answered.
    #[test]
    fn an_undecodable_auth_header_does_not_retain_its_stream() {
        let (mut auth_io, port) = auth_io();

        for _ in 0..16 {
            send_request(
                port,
                &format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{}Authorization: !!!not base64!!!\r\n\r\n",
                    fingerprint_line()
                ),
            );
            drain_one(&mut auth_io);
        }

        assert!(auth_io.outgoing_streams.is_empty());
    }

    /// A single `read` is not guaranteed to deliver a whole request. A client
    /// whose headers arrive in two TCP segments must still be understood --
    /// reading once and parsing whatever happened to show up dropped it.
    #[test]
    fn a_request_split_across_two_writes_is_still_understood() {
        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n")
            .unwrap();
        stream.flush().unwrap();

        // Let the server see the first half on its own.
        for _ in 0..8 {
            assert!(matches!(auth_io.receive(), Ok(None)));
            thread::sleep(Duration::from_millis(1));
        }

        stream
            .write_all(format!("{}Authorization: {encoded}\r\n\r\n", fingerprint_line()).as_bytes())
            .unwrap();
        stream.flush().unwrap();

        assert!(drain_one(&mut auth_io), "the request should reach the app");
        assert_eq!(auth_io.outgoing_streams.len(), 1);
    }

    /// A rejection may carry a message explaining itself (naia-lib/naia#133).
    /// It rides base64-encoded in the 401 body so that every transport can
    /// carry it identically, including the ones whose bodies are text.
    #[test]
    fn a_rejection_carries_its_reason_in_the_body() {
        use std::io::Read;

        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{}Authorization: {encoded}\r\n\r\n",
                    fingerprint_line()
                )
                .as_bytes(),
            )
            .unwrap();
        stream.flush().unwrap();
        assert!(drain_one(&mut auth_io), "the request should reach the app");

        let addr = *auth_io.outgoing_streams.keys().next().unwrap();
        let reason = b"you are banned".to_vec();
        auth_io.reject(&addr, Some(&reason)).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        let body = response.split("\r\n\r\n").nth(1).unwrap();
        assert_eq!(base64::decode(body.trim()).unwrap(), reason);
    }

    /// A rejection with no reason leaves the body empty, which the client reads
    /// back as "no reason given" rather than as a malformed message.
    #[test]
    fn a_rejection_without_a_reason_has_an_empty_body() {
        use std::io::Read;

        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{}Authorization: {encoded}\r\n\r\n",
                    fingerprint_line()
                )
                .as_bytes(),
            )
            .unwrap();
        stream.flush().unwrap();
        assert!(drain_one(&mut auth_io), "the request should reach the app");

        let addr = *auth_io.outgoing_streams.keys().next().unwrap();
        auth_io.reject(&addr, None).unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        assert_eq!(response.split("\r\n\r\n").nth(1).unwrap(), "");
    }

    /// A fingerprint mismatch gets a prompt, payload-free, reserved-status
    /// answer -- not a silent drop -- so the client fails fast with exactly
    /// one `RejectEvent(ProtocolMismatch)`. Absent and wrong-value take one
    /// indistinguishable branch carrying neither fingerprint, and neither
    /// reaches the application.
    #[test]
    fn a_fingerprint_mismatch_gets_a_prompt_reserved_answer() {
        use std::io::Read;

        let encoded = base64::encode([1u8, 2, 3, 4]);
        let wrong = ProtocolId::new(1).to_hex();
        let mut first: Option<String> = None;

        for (label, fingerprint_opt) in [("absent", None), ("wrong value", Some(wrong.clone()))] {
            let (mut auth_io, port) = auth_io();

            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let fingerprint_headers = fingerprint_opt
                .as_ref()
                .map(|f| format!("{}: {}\r\n", PROTOCOL_ID_HEADER, f))
                .unwrap_or_default();
            stream
                .write_all(
                    format!(
                        "GET / HTTP/1.1\r\nHost: localhost\r\n{}Authorization: {encoded}\r\n\r\n",
                        fingerprint_headers
                    )
                    .as_bytes(),
                )
                .unwrap();
            stream.flush().unwrap();

            assert!(
                !drain_one(&mut auth_io),
                "a {label} fingerprint must not reach the application",
            );
            assert!(
                auth_io.outgoing_streams.is_empty(),
                "a {label} fingerprint must not retain a stream for the app",
            );

            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();

            assert!(
                response.starts_with(&format!("HTTP/1.1 {}", PROTOCOL_MISMATCH_STATUS)),
                "a {label} fingerprint must get the reserved mismatch status, got: {response}",
            );
            assert!(
                !response.starts_with("HTTP/1.1 401"),
                "a mismatch must not look like an application rejection",
            );
            let body = response.split("\r\n\r\n").nth(1).unwrap();
            assert_eq!(body, "", "the mismatch answer must be payload-free");
            assert!(
                !response.contains(&expected_id().to_hex()),
                "the mismatch answer must never echo the expected fingerprint",
            );
            assert!(
                fingerprint_opt
                    .as_ref()
                    .map(|f| !response.contains(f))
                    .unwrap_or(true),
                "the mismatch answer must never echo the received fingerprint",
            );

            match &first {
                None => first = Some(response),
                Some(first) => assert_eq!(
                    first, &response,
                    "a {label} fingerprint must be indistinguishable on the wire",
                ),
            }
        }

        // The unit-level splitter agrees: these are mismatches, while a
        // fingerprinted-but-credentialless request is merely malformed.
        let good = expected_id().to_hex();
        let mismatched = format!(
            "GET / HTTP/1.1\r\nHost: x\r\n{}: {}\r\nAuthorization: {}\r\n\r\n",
            PROTOCOL_ID_HEADER,
            ProtocolId::new(1).to_hex(),
            base64::encode([9u8]),
        );
        assert!(fingerprint_mismatch(mismatched.as_bytes(), &expected_id()));
        let absent = format!(
            "GET / HTTP/1.1\r\nHost: x\r\nAuthorization: {}\r\n\r\n",
            base64::encode([9u8]),
        );
        assert!(fingerprint_mismatch(absent.as_bytes(), &expected_id()));
        let matched = format!(
            "GET / HTTP/1.1\r\nHost: x\r\n{}: {}\r\nAuthorization: {}\r\n\r\n",
            PROTOCOL_ID_HEADER,
            good,
            base64::encode([9u8]),
        );
        assert!(!fingerprint_mismatch(matched.as_bytes(), &expected_id()));
        assert!(decode_auth_request(matched.as_bytes(), &expected_id()).is_some());
    }

    /// The listener is non-blocking, but the streams it accepts are not, so a
    /// peer that connects and then says nothing used to block `receive` -- and
    /// with it the server's whole tick -- indefinitely.
    #[test]
    fn a_peer_that_sends_nothing_does_not_stall_the_server() {
        let (mut auth_io, port) = auth_io();

        let _silent = TcpStream::connect(("127.0.0.1", port)).unwrap();

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for _ in 0..8 {
                let _ = auth_io.receive();
            }
            let _ = tx.send(());
        });

        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "receive must not block on a peer that has sent nothing",
        );
    }

    /// Half-read requests are themselves free for a peer to create, so the set
    /// of them needs its own ceiling.
    #[test]
    fn the_pending_read_backlog_is_capped() {
        let (mut auth_io, port) = auth_io();

        // Drained as they are made: the listener's accept backlog is far
        // smaller than the number of connections opened here, so leaving them
        // all queued in the kernel would just stall the test's own `connect`.
        let mut silent = Vec::new();
        for _ in 0..MAX_PENDING_AUTH_READS + 64 {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                silent.push(stream);
            }
            assert!(matches!(auth_io.receive(), Ok(None)));
        }

        assert_eq!(
            auth_io.pending_reads.len(),
            MAX_PENDING_AUTH_READS,
            "half-read requests must not accumulate past the ceiling",
        );
    }

    /// A real auth request still retains its stream -- `accept`/`reject` need
    /// it to answer the client.
    #[test]
    fn a_real_auth_request_retains_its_stream_for_the_reply() {
        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        send_request(
            port,
            &format!(
                "GET / HTTP/1.1\r\nHost: localhost\r\n{}Authorization: {encoded}\r\n\r\n",
                fingerprint_line()
            ),
        );

        assert!(drain_one(&mut auth_io), "the request should reach the app");
        assert_eq!(auth_io.outgoing_streams.len(), 1);
    }

    // ---- protocol fingerprint gate -----------------------------------------

    /// Every way of failing the fingerprint check, as a peer can produce them.
    fn refused_fingerprints() -> Vec<(&'static str, String)> {
        let good = expected_id().to_hex();
        vec![
            ("absent", String::new()),
            (
                "wrong value",
                format!(
                    "{}: {}\r\n",
                    PROTOCOL_ID_HEADER,
                    ProtocolId::new(1).to_hex()
                ),
            ),
            (
                "too short",
                format!("{}: {}\r\n", PROTOCOL_ID_HEADER, &good[..good.len() - 1]),
            ),
            ("too long", format!("{}: {}0\r\n", PROTOCOL_ID_HEADER, good)),
            (
                "not hex",
                format!("{}: {}zz\r\n", PROTOCOL_ID_HEADER, &good[..good.len() - 2]),
            ),
            ("empty value", format!("{}: \r\n", PROTOCOL_ID_HEADER)),
        ]
    }

    /// F9/F10/F13 -- **A peer running a different protocol gets nothing.**
    ///
    /// The request below carries a perfectly good `Authorization` header. It
    /// still must not reach the application: no auth event, no user record, no
    /// identity token, and -- because the gate runs above the base64 decode --
    /// the credential itself is never even decoded, so a single-use token
    /// presented by a mismatching peer is not consumed and remains usable by
    /// the peer it was issued to.
    ///
    /// All six failure shapes are asserted together, and asserted to produce
    /// the *same* outcome, because a difference between them is exactly the
    /// distinction a probing peer would use to learn what the server expects.
    #[test]
    fn an_auth_request_failing_the_fingerprint_check_never_reaches_the_application() {
        let encoded = base64::encode([1u8, 2, 3, 4]);

        for (label, line) in refused_fingerprints() {
            let (mut auth_io, port) = auth_io();

            send_request(
                port,
                &format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{line}Authorization: {encoded}\r\n\r\n"
                ),
            );

            assert!(
                !drain_one(&mut auth_io),
                "a request with a {label} fingerprint must not reach the application",
            );
            assert!(
                auth_io.outgoing_streams.is_empty(),
                "a request with a {label} fingerprint must not retain a stream",
            );
        }
    }

    /// The gate is above the credential, not beside it: `decode_auth_request`
    /// returns `None` for a mismatching peer *whatever* its `Authorization`
    /// header says, including when there is none at all. Asserted at this level
    /// as well as through the socket because it is the property the rest of the
    /// auth path depends on -- if this function ever decoded first, the
    /// consume-the-token failure would be invisible from outside.
    #[test]
    fn the_fingerprint_is_compared_before_the_credential_is_decoded() {
        let expected = expected_id();
        let encoded = base64::encode([1u8, 2, 3, 4]);
        let good = fingerprint_line();

        // Baseline: with the right fingerprint, the credential comes back.
        let accepted =
            format!("GET / HTTP/1.1\r\nHost: localhost\r\n{good}Authorization: {encoded}\r\n\r\n");
        assert_eq!(
            decode_auth_request(accepted.as_bytes(), &expected),
            Some(vec![1u8, 2, 3, 4]),
        );

        // Every refusal shape yields `None`, and so does the same refusal with
        // no credential at all -- one outcome, no observable difference.
        for (label, line) in refused_fingerprints() {
            for credential in [format!("Authorization: {encoded}\r\n"), String::new()] {
                let request =
                    format!("GET / HTTP/1.1\r\nHost: localhost\r\n{line}{credential}\r\n");
                assert_eq!(
                    decode_auth_request(request.as_bytes(), &expected),
                    None,
                    "{label} must be refused regardless of the credential beside it",
                );
            }
        }
    }

    /// The *ordering* itself, asserted on the source.
    ///
    /// The test above pins the outcome, and the outcome is the same whichever
    /// order the two checks run in: a mismatching peer gets `None` either way.
    /// That is the fail-closed guarantee working, and it is precisely why
    /// behaviour cannot falsify a reordering here. The contract nevertheless
    /// requires the comparison to happen *before* the credential is read and
    /// base64-decoded -- an attacker who fails the gate must not have caused any
    /// decoding work -- so the ordering is asserted where it is visible: in the
    /// text of the function.
    ///
    /// The function body is sliced out first so the assertion cannot be
    /// satisfied by this test's own source, or by any other function in the
    /// file.
    #[test]
    fn the_gate_precedes_the_credential_in_the_source_not_just_in_the_outcome() {
        const THIS_FILE: &str = include_str!("udp.rs");

        let signature = "fn decode_auth_request(";
        let start = THIS_FILE
            .find(signature)
            .expect("decode_auth_request must exist");
        let body = &THIS_FILE[start..];
        let end = body
            .find("\n}\n")
            .expect("decode_auth_request must be closed");
        let body = &body[..end];

        let gate = body
            .find(PROTOCOL_ID_HEADER_CONST)
            .expect("decode_auth_request must consult the fingerprint header");
        let credential = body
            .find(r#"get("Authorization")"#)
            .expect("decode_auth_request must read the credential header");

        assert!(
            gate < credential,
            "the fingerprint must be compared before the credential is read and decoded",
        );
    }

    /// The identifier the test above looks for, kept out of the assertion so
    /// the literal appears once.
    const PROTOCOL_ID_HEADER_CONST: &str = "PROTOCOL_ID_HEADER";

    /// A refusal must not tell the peer what the server expects. The gate has
    /// no response path of its own -- it drops the connection -- so this
    /// asserts the absence of the one thing that could leak: the expected
    /// value never appears in anything written back.
    #[test]
    fn a_refusal_does_not_echo_the_expected_fingerprint() {
        use std::io::Read;

        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                format!(
                    "GET / HTTP/1.1\r\nHost: localhost\r\n{}: {}\r\nAuthorization: {encoded}\r\n\r\n",
                    PROTOCOL_ID_HEADER,
                    ProtocolId::new(1).to_hex(),
                )
                .as_bytes(),
            )
            .unwrap();
        stream.flush().unwrap();

        assert!(!drain_one(&mut auth_io));

        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);

        assert!(
            !response.contains(&expected_id().to_hex()),
            "the expected fingerprint must never be echoed; got: {response}",
        );
    }
}
