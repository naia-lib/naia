extern crate log;

use naia_socket_shared::SocketConfig;

use log::info;

use crate::{
    conditioned_packet_receiver::ConditionedPacketReceiver,
    io::Io,
    packet_receiver::{PacketReceiver, PacketReceiverTrait},
};

use super::{
    packet_receiver::PacketReceiverImpl, packet_sender::PacketSender,
    peer_connection::PeerConnection,
};

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

        let mut peer_connection = PeerConnection::new(&self.config, server_session_url);
        peer_connection.on_find_addr(Box::new(move |socket_addr| {
            info!("found socket_addr: {:?}", socket_addr);
        }));
        let data_port = peer_connection.data_port();
        let addr_cell = peer_connection.addr_cell();

        let packet_sender = PacketSender::new(data_port.message_port.clone(), addr_cell.clone());
        let packet_receiver_impl = PacketReceiverImpl::new(data_port.message_queue.clone(), addr_cell);

        let packet_receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &self.config.link_condition {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        self.io = Some(Io {
            packet_sender,
            packet_receiver: PacketReceiver::new(packet_receiver),
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

unsafe impl Send for Socket {}
unsafe impl Sync for Socket {}
