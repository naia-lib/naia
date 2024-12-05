use std::{
    io::{Read, Write, ErrorKind},
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use naia_shared::{IdentityToken, LinkConditionerConfig};

use super::{conditioner::ConditionedPacketReceiver, IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportAddr, Socket as TransportSocket};

// Socket
pub struct Socket {
    server_addr: SocketAddr,
    udp_socket: Arc<Mutex<UdpSocket>>,
    auth_io: Arc<Mutex<AuthIo>>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(server_addr: &SocketAddr, config: Option<LinkConditionerConfig>) -> Self {
        let client_ip_address =
            find_my_ip_address().expect("cannot find host's current IP address");

        let udp_socket = Arc::new(Mutex::new(UdpSocket::bind((client_ip_address, 0)).unwrap()));
        udp_socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        let auth_io = Arc::new(Mutex::new(AuthIo::new(server_addr)));

        Self {
            server_addr: *server_addr,
            udp_socket,
            auth_io,
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

        let packet_sender = Box::new(PacketSender::new(self.udp_socket.clone(), self.server_addr));

        let packet_receiver = UdpPacketReceiver::new(self.udp_socket.clone(), self.server_addr);
        let packet_receiver: Box<dyn PacketReceiver> = {
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(packet_receiver, config))
            } else {
                Box::new(packet_receiver)
            }
        };

        return (
            Box::new(id_receiver),
            packet_sender,
            packet_receiver
        );
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
        return Self {
            socket,
            server_addr,
        };
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
        return Ok(());
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
        return Self {
            socket,
            server_addr,
            buffer: [0; 1472],
        };
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
                        return Ok(None);
                    }
                    _ => {
                        return Err(RecvError);
                    }
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
    pub fn new(server_addr: &SocketAddr) -> Self {
        Self {
            server_addr: *server_addr,
            buffer: [0; 1472],
            stream_opt: None,
        }
    }

    fn connect(
        &mut self,
        auth_bytes_opt: Option<Vec<u8>>,
        auth_headers_opt: Option<Vec<(String, String)>>,
    ) {
        let mut stream = TcpStream::connect(self.server_addr).unwrap();
        stream.set_nonblocking(true).unwrap();

        let sig: u8 = match (auth_bytes_opt.is_some(), auth_headers_opt.is_some()) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        };

        stream.write(&[sig]).unwrap();

        if let Some(auth_bytes) = auth_bytes_opt {
            stream.write(&auth_bytes).unwrap();
            stream.write(b"\r\n").unwrap();
        }
        if let Some(auth_headers) = auth_headers_opt {
            stream.write(&[1]).unwrap();
            for (key, value) in auth_headers {
                stream.write(key.as_bytes()).unwrap();
                stream.write(b": ").unwrap();
                stream.write(value.as_bytes()).unwrap();
                stream.write(b"\r\n").unwrap();
            }
        }

        stream.flush().unwrap();

        self.stream_opt = Some(stream);
    }

    fn receive(&mut self) -> IdentityReceiverResult {
        let Some(stream) = self.stream_opt.as_mut() else {
            panic!("No stream to receive from (did you forget to call connect?)");
        };
        match stream.read(&mut self.buffer) {
            Ok(recv_len) => {

                // read first byte to determine if was succesfull or not
                let success_byte = self.buffer[0];
                if success_byte == 0 {
                    return IdentityReceiverResult::ErrorResponseCode(401);
                }
                if success_byte != 1 {
                    warn!("Unexpected id response type: {}", success_byte);
                    return IdentityReceiverResult::ErrorResponseCode(500);
                }

                // read the rest of the bytes as the identity token

                let id_token = IdentityToken::from_utf8_lossy(&self.buffer[1..recv_len]).to_string();
                return IdentityReceiverResult::Success(id_token);
            }
            Err(ref e) => {
                let kind = e.kind();
                match kind {
                    ErrorKind::WouldBlock => {
                        IdentityReceiverResult::Waiting
                    }
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

//

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpStream};

use log::warn;

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
