extern crate log;

use std::{
    net::UdpSocket,
    sync::{Arc, Mutex},
};

use log::info;

use naia_socket_shared::{parse_server_url, url_to_socket_addr, SocketConfig};

use crate::{
    conditioned_packet_receiver::ConditionedPacketReceiver,
    io::Io,
    packet_receiver::{PacketReceiver, PacketReceiverTrait},
};

use super::{packet_receiver::PacketReceiverImpl, packet_sender::PacketSender};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket {
    config: SocketConfig,
    io: Option<Io>,
}

impl Socket {
    /// Create a new Socket
    pub fn new(config: &SocketConfig) -> Self {
        Socket {
            config: config.clone(),
            io: None,
        }
    }

    /// Connects to the given server address
    pub fn connect(&mut self, server_session_url: &str) {
        if self.io.is_some() {
            panic!("Socket already listening!");
        }

        let server_url = parse_server_url(server_session_url);
        let server_socket_addr = url_to_socket_addr(&server_url);

        let client_ip_address =
            find_my_ip_address().expect("cannot find host's current IP address");

        let socket = Arc::new(Mutex::new(UdpSocket::bind((client_ip_address, 0)).unwrap()));
        socket
            .as_ref()
            .lock()
            .unwrap()
            .set_nonblocking(true)
            .expect("can't set socket to non-blocking!");
        let local_addr = socket.as_ref().lock().unwrap().local_addr().unwrap();

        let packet_sender = PacketSender::new(server_socket_addr, socket.clone());

        let conditioner_config = self.config.link_condition.clone();

        let receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(PacketReceiverImpl::new(server_socket_addr, socket));
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        info!("UDP client listening on socket: {}", local_addr);

        self.io = Some(Io {
            packet_sender,
            packet_receiver: PacketReceiver::new(receiver),
        });
    }

    /// Gets a PacketSender which can be used to send packets through the Socket
    pub fn packet_sender(&self) -> PacketSender {
        return self
            .io
            .as_ref()
            .expect("Socket is not connected yet! Call Socket.connect() before this.")
            .packet_sender
            .clone();
    }

    /// Gets a PacketReceiver which can be used to receive packets from the
    /// Socket
    pub fn packet_receiver(&self) -> PacketReceiver {
        return self
            .io
            .as_ref()
            .expect("Socket is not connected yet! Call Socket.connect() before this.")
            .packet_receiver
            .clone();
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
