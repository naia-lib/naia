use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{Arc, Mutex},
};

use naia_shared::{transport_udp, IdentityToken, LinkConditionerConfig};

use super::{
    conditioner::ConditionedPacketReceiver, AuthReceiver as TransportAuthReceiver,
    AuthSender as TransportAuthSender, PacketReceiver, PacketSender as TransportSender, RecvError,
    SendError, Socket as TransportSocket,
};
use crate::user::UserAuthAddr;

// Socket
pub struct Socket {
    data_socket: Arc<Mutex<UdpSocket>>,
    auth_io: Arc<Mutex<AuthIo>>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
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
            .unwrap()
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
    fn listen(
        self: Box<Self>,
    ) -> (
        Box<dyn TransportAuthSender>,
        Box<dyn TransportAuthReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>,
    ) {
        let auth_sender = AuthSender::new(self.auth_io.clone());
        let auth_receiver = AuthReceiver::new(self.auth_io.clone());
        let packet_sender = UdpPacketSender::new(self.data_socket.clone());
        let packet_receiver = UdpPacketReceiver::new(self.data_socket.clone());

        let packet_receiver: Box<dyn PacketReceiver> = {
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(packet_receiver, config))
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
            .unwrap()
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
        match self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .recv_from(&mut self.buffer)
        {
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

    fn receive(&mut self) -> Result<Option<(UserAuthAddr, &[u8])>, RecvError> {
        match self.socket.accept() {
            Ok((mut stream, addr)) => {
                let recv_len = stream.read(&mut self.buffer).unwrap();
                if self.outgoing_streams.contains_key(&addr) {
                    // already have a stream for this address
                    // TODO: handle this case?
                    return Err(RecvError);
                }
                self.outgoing_streams.insert(addr, stream);

                let request = transport_udp::bytes_to_request(&self.buffer[..recv_len]);
                if request.headers().contains_key("Authorization") {
                    let auth_bytes = request
                        .headers()
                        .get("Authorization")
                        .unwrap()
                        .to_str()
                        .unwrap();
                    let auth_bytes = base64::decode(auth_bytes).unwrap();
                    self.buffer[0..auth_bytes.len()].copy_from_slice(&auth_bytes);
                    return Ok(Some((
                        UserAuthAddr::new(addr),
                        &self.buffer[..auth_bytes.len()],
                    )));
                } else {
                    return Ok(None);
                }
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
        address: &UserAuthAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(&address.addr()) {
            let response_body = format!("{}\r\n{}", identity_token, self.public_udp_addr);
            let response_body_bytes = response_body.into_bytes();

            let response = http::Response::builder()
                .status(200)
                .body(response_body_bytes)
                .unwrap();
            let response_bytes = transport_udp::response_to_bytes(response);
            stream.write_all(&response_bytes).unwrap();

            stream.flush().unwrap();

            return Ok(());
        }
        Err(SendError)
    }

    /// Sends a rejection packet from the Client Socket
    fn reject(&mut self, address: &UserAuthAddr) -> Result<(), SendError> {
        if let Some(mut stream) = self.outgoing_streams.remove(&address.addr()) {
            let response = http::Response::builder()
                .status(401)
                .body(Vec::new())
                .unwrap();
            let response_bytes = transport_udp::response_to_bytes(response);
            stream.write_all(&response_bytes).unwrap();

            stream.flush().unwrap();

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
        address: &UserAuthAddr,
        identity_token: &IdentityToken,
    ) -> Result<(), SendError> {
        self.auth_io.lock().unwrap().accept(address, identity_token)
    }

    /// Sends a rejection packet from the Client Socket
    fn reject(&self, address: &UserAuthAddr) -> Result<(), SendError> {
        self.auth_io.lock().unwrap().reject(address)
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
    fn receive(&mut self) -> Result<Option<(UserAuthAddr, &[u8])>, RecvError> {
        let mut guard = self.auth_io.lock().unwrap();
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
    let url = Url::parse(url_str).expect("server_url_str is not a valid URL!");
    if let Some(path_segments) = url.path_segments() {
        let path_segment_count = path_segments.count();
        if path_segment_count > 1 {
            log::error!("server_url_str must not include a path");
            panic!("");
        }
    }
    if url.query().is_some() {
        log::error!("server_url_str must not include a query string");
        panic!("");
    }
    if url.fragment().is_some() {
        log::error!("server_url_str must not include a fragment");
        panic!("");
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
                log::error!("{}", SOCKET_PARSE_FAIL_STR);
                panic!("");
            }

            return *addr_list.first().expect(SOCKET_PARSE_FAIL_STR);
        }
        Err(err) => {
            log::error!("URL -> SocketAddr parse fails with: {:?}", err);
            panic!("");
        }
    }
}
