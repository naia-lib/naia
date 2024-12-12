use std::{
    io::ErrorKind,
    net::{UdpSocket, IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{Arc, Mutex},
};

use naia_shared::LinkConditionerConfig;

use crate::transport::{
    IdentityReceiver, PacketReceiver,
    PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportAddr,
    Socket as TransportSocket,
    udp::{addr_cell::AddrCell, auth::{AuthIo, AuthReceiver}, conditioner::ConditionedPacketReceiver},
};

// Socket
pub struct Socket {
    auth_io: Arc<Mutex<AuthIo>>,

    data_addr_cell: AddrCell,
    data_socket: Arc<Mutex<UdpSocket>>,

    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(server_session_url: &str, config: Option<LinkConditionerConfig>) -> Self {

        let data_addr_cell = AddrCell::default();
        let auth_io = Arc::new(Mutex::new(AuthIo::new(data_addr_cell.clone(), server_session_url)));

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
            data_addr_cell,
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

        let packet_sender = Box::new(PacketSender::new(self.data_addr_cell.clone(), self.data_socket.clone()));

        let packet_receiver = UdpPacketReceiver::new(self.data_addr_cell.clone(), self.data_socket.clone());
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
    addr_cell: AddrCell,
}

impl PacketSender {
    pub fn new(addr_cell: AddrCell, socket: Arc<Mutex<UdpSocket>>) -> Self {
        Self {
            socket,
            addr_cell,
        }
    }
}

impl TransportSender for PacketSender {
    /// Sends a packet from the Client Socket
    fn send(&self, payload: &[u8]) -> Result<(), SendError> {
        let TransportAddr::Found(server_addr) = self.server_addr() else {
            return Err(SendError);
        };
        if self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .send_to(payload, server_addr)
            .is_err()
        {
            return Err(SendError);
        }
        Ok(())
    }
    /// Get the Server's Socket address
    fn server_addr(&self) -> TransportAddr {
        self.addr_cell.get()
    }
}

// Packet Receiver
#[derive(Clone)]
pub(crate) struct UdpPacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    addr_cell: AddrCell,
    buffer: [u8; 1472],
}

impl UdpPacketReceiver {
    pub fn new(addr_cell: AddrCell, socket: Arc<Mutex<UdpSocket>>) -> Self {
        Self {
            socket,
            addr_cell,
            buffer: [0; 1472],
        }
    }
}

impl PacketReceiver for UdpPacketReceiver {
    /// Receives a packet from the Client Socket
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        let TransportAddr::Found(server_addr) = self.server_addr() else {
            return Ok(None);
        };
        match self
            .socket
            .as_ref()
            .lock()
            .unwrap()
            .recv_from(&mut self.buffer)
        {
            Ok((recv_len, address)) => {
                if address == server_addr {
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
        self.addr_cell.get()
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