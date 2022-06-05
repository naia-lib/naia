extern crate log;

use std::net::SocketAddr;

use naia_socket_shared::SocketConfig;

use crate::{
    conditioned_packet_receiver::ConditionedPacketReceiver,
    io::Io,
    packet_receiver::{PacketReceiver, PacketReceiverTrait},
};

use super::{
    addr_cell::AddrCell, data_channel::DataChannel, data_port::DataPort,
    packet_receiver::PacketReceiverImpl, packet_sender::PacketSender,
};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket {
    config: SocketConfig,
    server_addr: AddrCell,
    io: Option<Io>,
}

impl Socket {
    /// Create a new Socket
    pub fn new(config: &SocketConfig) -> Self {
        Socket {
            config: config.clone(),
            io: None,
            server_addr: AddrCell::default(),
        }
    }

    /// Connects to the given server address
    pub fn connect(&mut self, server_session_url: &str) {
        if self.io.is_some() {
            panic!("Socket already listening!");
        }

        let data_channel = DataChannel::new(&self.config, server_session_url);

        let data_port = data_channel.data_port();
        self.server_addr = data_channel.addr_cell();

        self.setup_io(&data_port);

        data_channel.start();
    }

    // Creates a Socket from an underlying DataPort.
    // This is for use in apps running within a Web Worker.
    pub fn connect_with_data_port(&mut self, data_port: &DataPort) {
        if self.io.is_some() {
            panic!("Socket already listening!");
        }

        self.setup_io(data_port);
    }

    // Sets the socket address associated with the Server.
    // This is for use in apps running within a Web Worker.
    pub fn set_server_addr(&mut self, socket_addr: &SocketAddr) {
        self.server_addr.set_addr(socket_addr);
    }

    fn setup_io(&mut self, data_port: &DataPort) {
        if self.io.is_some() {
            panic!("Socket already listening!");
        }

        let packet_sender = PacketSender::new(&data_port, &self.server_addr);
        let packet_receiver_impl = PacketReceiverImpl::new(&data_port, &self.server_addr);

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
