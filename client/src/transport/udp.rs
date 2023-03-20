use std::{
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use naia_shared::LinkConditionerConfig;

use super::{
    conditioner::ConditionedPacketReceiver, PacketReceiver as TransportReceiver,
    PacketSender as TransportSender, RecvError, SendError, ServerAddr as TransportAddr,
    Socket as TransportSocket,
};

// Socket
pub struct Socket {
    server_addr: SocketAddr,
    socket: Arc<Mutex<UdpSocket>>,
    config: Option<LinkConditionerConfig>,
}

impl Socket {
    pub fn new(server_addr: &SocketAddr, config: Option<LinkConditionerConfig>) -> Self {
        let client_ip_address =
            find_my_ip_address().expect("cannot find host's current IP address");

        let socket = Arc::new(Mutex::new(UdpSocket::bind((client_ip_address, 0)).unwrap()));
        socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");

        return Self {
            server_addr: *server_addr,
            socket,
            config,
        };
    }
}

impl Into<Box<dyn TransportSocket>> for Socket {
    fn into(self) -> Box<dyn TransportSocket> {
        Box::new(self)
    }
}

impl TransportSocket for Socket {
    fn connect(self: Box<Self>) -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let sender = Box::new(PacketSender::new(self.socket.clone(), self.server_addr));

        let receiver: Box<dyn TransportReceiver> = {
            let inner_receiver =
                Box::new(PacketReceiver::new(self.socket.clone(), self.server_addr));
            if let Some(config) = &self.config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        return (sender, receiver);
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
struct PacketReceiver {
    socket: Arc<Mutex<UdpSocket>>,
    server_addr: SocketAddr,
    buffer: [u8; 1472],
}

impl PacketReceiver {
    pub fn new(socket: Arc<Mutex<UdpSocket>>, server_addr: SocketAddr) -> Self {
        return Self {
            socket,
            server_addr,
            buffer: [0; 1472],
        };
    }
}

impl TransportReceiver for PacketReceiver {
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

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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
