use std::{
    io::{Read, Write, ErrorKind},
    net::{SocketAddr, UdpSocket, IpAddr, Ipv4Addr, Ipv6Addr, TcpStream},
    sync::{Arc, Mutex},
};

use log::warn;

use naia_shared::{transport_udp, IdentityToken, LinkConditionerConfig};

use super::{conditioner::ConditionedPacketReceiver, IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportAddr, Socket as TransportSocket};

// Socket
pub struct Socket {
    auth_io: Arc<Mutex<AuthIo>>,

    data_addr: SocketAddr,
    data_socket: Arc<Mutex<UdpSocket>>,

    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(
        auth_addr: &SocketAddr,
        data_addr: &SocketAddr,
        config: Option<LinkConditionerConfig>,
    ) -> Self {

        let auth_io = Arc::new(Mutex::new(AuthIo::new(auth_addr)));

        let client_ip_address =
            find_my_ip_address().expect("cannot find host's current IP address");

        let data_socket = Arc::new(Mutex::new(UdpSocket::bind((client_ip_address, 0)).unwrap()));
        data_socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        Self {
            auth_io,
            data_addr: *data_addr,
            data_socket,
            config,
        }
    }

    fn connect_inner(
        self: Box<Self>,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>
    ) {
        self.auth_io.lock().unwrap().connect(auth_bytes_opt, auth_headers_opt);
        let id_receiver = AuthReceiver::new(self.auth_io.clone());

        let packet_sender = Box::new(PacketSender::new(self.data_socket.clone(), self.data_addr));

        let packet_receiver = UdpPacketReceiver::new(self.data_socket.clone(), self.data_addr);
        let packet_receiver: Box<dyn PacketReceiver> = {
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(packet_receiver, config))
            } else {
                Box::new(packet_receiver)
            }
        };

        (
            Box::new(id_receiver),
            packet_sender,
            packet_receiver
        )
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn connect(
        self: Box<Self>
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>
    ) {
        self.connect_inner(None, None)
    }

    fn connect_with_auth(
        self: Box<Self>,
        auth_bytes: Vec<u8>
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>
    ) {
        self.connect_inner(Some(auth_bytes), None)
    }

    fn connect_with_auth_headers(
        self: Box<Self>,
        auth_headers: Vec<(String, String)>
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>
    ) {
        self.connect_inner(None, Some(auth_headers))
    }

    fn connect_with_auth_and_headers(
        self: Box<Self>,
        auth_bytes: Vec<u8>,
        auth_headers: Vec<(String, String)>
    ) -> (
        Box<dyn IdentityReceiver>,
        Box<dyn TransportSender>,
        Box<dyn PacketReceiver>
    ) {
        self.connect_inner(Some(auth_bytes), Some(auth_headers))
    }
}

// Packet Sender
struct PacketSender {
    socket: Arc<Mutex<UdpSocket>>,
    server_addr: SocketAddr,
}

impl PacketSender {
    pub fn new(socket: Arc<Mutex<UdpSocket>>, server_addr: SocketAddr) -> Self {
        Self {
            socket,
            server_addr,
        }
    }
}

impl TransportSender for PacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        if self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .send_to(payload, self.server_addr)
            .is_err()
        {
            return Err(SendError);
        }
        Ok(())
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        TransportAddr::Found(self.server_addr)
    }
}

// Packet Receiver
#[derive(Clone)]
pub(crate) struct UdpPacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    server_addr: SocketAddr,
    buffer: [u8; 1472],
}

impl UdpPacketReceiver {
    pub fn new(socket: Arc<Mutex<UdpSocket>>, server_addr: SocketAddr) -> Self {
        Self {
            socket,
            server_addr,
            buffer: [0; 1472],
        }
    }
}

impl PacketReceiver for UdpPacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        match self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .recv_from(&mut self.buffer)
        {
            Ok((recv_len, address)) => {
                if address == self.server_addr {
                    Ok(Some(&self.buffer[..recv_len]))
                } else {
                    Err(RecvError)
                }
            }
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => {
                        //just didn't receive anything this time
                        Ok(None)
                    }
                    _ => Err(RecvError),
                }
            }
        }
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        TransportAddr::Found(self.server_addr)
    }
}

// AuthIo

pub(crate) struct AuthIo {
    server_addr: SocketAddr,
    buffer: [u8; 1472],
    stream_opt: Option<TcpStream>,
}

impl AuthIo {
    pub fn new(addr: &SocketAddr) -> Self {

        Self {
            server_addr: *addr,
            buffer: [0; 1472],
            stream_opt: None,
        }
    }

    fn connect(
        &mut self,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) {
        let mut stream = match TcpStream::connect(self.server_addr) {
            Ok(stream) => stream,
            Err(err) => {
                panic!("Couldn't connect to server at address: {:?}. {:?}", self.server_addr, err);
            }
        };
        stream.set_nonblocking(true).unwrap();

        let mut request = http::Request::builder()
            .method("POST")
            .uri("/");
        if let Some(auth_bytes) = auth_bytes_opt {
            let base64_encoded = base64::encode(&auth_bytes);
            request = request.header("Authorization", &base64_encoded);
        }
        if let Some(auth_headers) = auth_headers_opt.clone() {
            for (key, value) in auth_headers {
                request = request.header(key, value);
            }
        }
        let request = request.body(Vec::new()).unwrap();
        let request_bytes = transport_udp::request_to_bytes(request);
        stream.write_all(&request_bytes).unwrap();

        stream.flush().unwrap();

        self.stream_opt = Some(stream);
    }

    fn receive(&mut self) -> IdentityReceiverResult {
        let Some(stream) = self.stream_opt.as_mut() else {
            panic!("No stream to receive from (did you forget to call connect?)");
        };
        match stream.read(&mut self.buffer) {
            Ok(recv_len) => {

                let response = transport_udp::bytes_to_response(&self.buffer[..recv_len]);
                let response_status = response.status().as_u16();
                if response_status != 200 {
                    return IdentityReceiverResult::ErrorResponseCode(response_status);
                }

                // read the rest of the bytes as the identity token
                let id_token = IdentityToken::from_utf8_lossy(response.body()).to_string();
                IdentityReceiverResult::Success(id_token)
            }
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => IdentityReceiverResult::Waiting,
                    _ => {
                        warn!("Unexpected auth read error: {:?}",  e);
                        IdentityReceiverResult::ErrorResponseCode(500)
                    }
                }
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
            if guard.stream_opt.is_none() {
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

/// Helper method to find local IP address, if possible
pub fn find_my_ip_address() -> Option<IpAddr> {
    let ip = local_ipaddress::get().unwrap_or_default();

    if let Ok(addr) = ip.parse::<Ipv4Addr>() {
        Some(IpAddr::V4(addr))
    } else if let Ok(addr) = ip.parse::<Ipv6Addr>() {
        Some(IpAddr::V6(addr))
    } else {
        None
    }
}