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

        let (socket, io) = RTCSocket::new();
        get_runtime().spawn(async move { socket.connect(&server_session_string).await });

        // Setup Packet Sender & Receiver
        let packet_sender = PacketSender::new(io.addr_cell.clone(), io.to_server_sender);
        let packet_receiver_impl = PacketReceiverImpl::new(io.addr_cell, io.to_client_receiver);

        let receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        self.io = Some(Io::new(
            packet_sender,
            PacketReceiver::new(receiver),
        ));
    }

    /// Returns whether or not the connect method was called (doesn't necessarily indicate that the
    /// connection is fully established).
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

    /// Gets a PacketReceiver which can be used to receive packets from the Socket
    pub fn packet_receiver(&mut self) -> PacketReceiver {
        return self
            .io
            .as_mut()
            .expect("Socket is not connected yet! Call Socket.connect() before this.")
            .packet_receiver
            .take()
            .expect("Can only call Socket.packet_receiver() once.");
    }
}
