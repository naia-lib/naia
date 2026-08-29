use parking_lot::Mutex;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::Arc,
};

use naia_shared::{http_utils, IdentityToken, LinkConditionerConfig};

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
    auth_io: Arc<Mutex<AuthIo>>,
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
        let auth_io = Arc::new(Mutex::new(AuthIo::new(
            &server_addrs.public_udp_url,
            auth_socket,
        )));

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
            auth_io,
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
    fn listen(self: Box<Self>) -> ListenResult {
        let auth_sender = AuthSender::new(self.auth_io.clone());
        let auth_receiver = AuthReceiver::new(self.auth_io.clone());
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
pub(crate) struct AuthIo {
    public_udp_addr: SocketAddr,
    socket: TcpListener,
    buffer: [u8; 1472],
    outgoing_streams: HashMap<SocketAddr, TcpStream>,
}

impl AuthIo {
    pub fn new(public_udp_url: &str, socket: TcpListener) -> Self {
        let public_udp_addr = url_str_to_addr(public_udp_url);

        Self {
            public_udp_addr,
            socket,
            buffer: [0; 1472],
            outgoing_streams: HashMap::new(),
        }
    }

    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match self.socket.accept() {
            Ok((mut stream, addr)) => {
                let recv_len = stream.read(&mut self.buffer).map_err(|_| RecvError)?;
                if self.outgoing_streams.contains_key(&addr) {
                    // already have a stream for this address
                    // TODO: handle this case?
                    return Err(RecvError);
                }

                // The stream is retained only once the request turns out to be
                // an auth request the application will be asked about, because
                // only `accept`/`reject` ever remove it again. Retaining it any
                // earlier leaked a live `TcpStream` -- an open socket, not just
                // memory -- for every connection an unauthenticated peer opened
                // and let fall through the checks below. Dropping `stream`
                // instead closes the connection.
                let auth_bytes = {
                    let request = http_utils::bytes_to_request(&self.buffer[..recv_len]);
                    let Some(auth_header) = request.headers().get("Authorization") else {
                        return Ok(None);
                    };
                    let auth_str = auth_header.to_str().map_err(|_| RecvError)?;
                    base64::decode(auth_str).map_err(|_| RecvError)?
                };

                self.outgoing_streams.insert(addr, stream);
                self.buffer[0..auth_bytes.len()].copy_from_slice(&auth_bytes);
                Ok(Some((addr, &self.buffer[..auth_bytes.len()])))
            }
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => Ok(None),
                    _ => Err(RecvError),
                }
            }
        }
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
    fn reject(&mut self, address: &SocketAddr) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(address) {
            let response = http::Response::builder()
                .status(401)
                .body(Vec::new())
                .map_err(|_| SendError)?;
            let response_bytes = http_utils::response_to_bytes(response);
            stream.write_all(&response_bytes).map_err(|_| SendError)?;
            stream.flush().map_err(|_| SendError)?;

            return Ok(());
        }
        Err(SendError)
    }
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
    fn reject(&self, address: &SocketAddr) -> Result<(), SendError> {
        self.auth_io.lock().reject(address)
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
        thread,
        time::Duration,
    };

    use super::AuthIo;

    fn auth_io() -> (AuthIo, u16) {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        socket.set_nonblocking(true).unwrap();
        (AuthIo::new("udp://127.0.0.1:14191", socket), port)
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
            send_request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
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
                "GET / HTTP/1.1\r\nHost: localhost\r\nAuthorization: !!!not base64!!!\r\n\r\n",
            );
            drain_one(&mut auth_io);
        }

        assert!(auth_io.outgoing_streams.is_empty());
    }

    /// A real auth request still retains its stream -- `accept`/`reject` need
    /// it to answer the client.
    #[test]
    fn a_real_auth_request_retains_its_stream_for_the_reply() {
        let (mut auth_io, port) = auth_io();
        let encoded = base64::encode([1u8, 2, 3, 4]);

        send_request(
            port,
            &format!("GET / HTTP/1.1\r\nHost: localhost\r\nAuthorization: {encoded}\r\n\r\n"),
        );

        assert!(drain_one(&mut auth_io), "the request should reach the app");
        assert_eq!(auth_io.outgoing_streams.len(), 1);
    }
}
