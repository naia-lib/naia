extern crate log;

use naia_socket_shared::{parse_server_url, SocketConfig};

use webrtc_unreliable_client::Socket as RTCSocket;

use crate::backends::native::runtime::get_runtime;
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
            panic!("Socket already connected!");
        }

        let server_session_string = format!(
            "{}{}",
            parse_server_url(server_session_url),
            self.config.rtc_endpoint_path.clone()
        );
        let conditioner_config = self.config.link_condition.clone();

        let (addr_cell, to_server_sender, to_client_receiver) =
            get_runtime().block_on(RTCSocket::connect(&server_session_string));

        // Setup Packet Sender & Receiver
        let packet_sender = PacketSender::new(addr_cell.clone(), to_server_sender);
        let packet_receiver_impl = PacketReceiverImpl::new(addr_cell, to_client_receiver);

        let receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        self.io = Some(Io {
            packet_sender,
            packet_receiver: PacketReceiver::new(receiver),
        });
    }

    /// Disconnects the Socket
    pub fn disconnect(&mut self) {
        if self.io.is_none() {
            panic!("Socket is currently disconnected!");
        }
        self.io = None;
        todo!()
    }

    /// Returns whether or not the Socket is currently connected to the server
    pub fn is_connected(&self) -> bool {
        self.io.is_some()
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
