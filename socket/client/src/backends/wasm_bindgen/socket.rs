use naia_socket_shared::SocketConfig;

use crate::{
    conditioned_packet_receiver::ConditionedPacketReceiver,
    packet_receiver::{PacketReceiver, PacketReceiverTrait},
    packet_sender::{PacketSender, PacketSenderTrait},
    backends::socket::SocketTrait,
};

use super::{
    addr_cell::AddrCell, data_channel::DataChannel, data_port::DataPort,
    packet_receiver::PacketReceiverImpl, packet_sender::PacketSenderImpl,
};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket;

impl Socket {
    /// Connects to the given server address
    pub fn connect(
        server_session_url: &str,
        config: &SocketConfig,
    ) -> (PacketSender, PacketReceiver) {
        let data_channel = DataChannel::new(config, server_session_url);

        let data_port = data_channel.data_port();
        let addr_cell = data_channel.addr_cell();

        let result = Socket::setup_io(config, &addr_cell, &data_port);

        data_channel.start();
        return result;
    }

    // Creates a Socket from an underlying DataPort.
    // This is for use in apps running within a Web Worker.
    pub fn connect_with_data_port(
        config: &SocketConfig,
        data_port: &DataPort,
    ) -> (PacketSender, PacketReceiver) {
        let addr_cell = AddrCell::new();
        return Socket::setup_io(config, &addr_cell, data_port);
    }

    fn setup_io(
        config: &SocketConfig,
        addr_cell: &AddrCell,
        data_port: &DataPort,
    ) -> (PacketSender, PacketReceiver) {
        // Setup Packet Sender
        let packet_sender_impl = PacketSenderImpl::new(&data_port, addr_cell);

        let packet_sender: Box<dyn PacketSenderTrait> = Box::new(packet_sender_impl);

        // Setup Packet Receiver
        let packet_receiver_impl = PacketReceiverImpl::new(&data_port, addr_cell);

        let packet_receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &config.link_condition {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };

        return (
            PacketSender::new(packet_sender),
            PacketReceiver::new(packet_receiver),
        );
    }
}

impl SocketTrait for Socket {
    /// Connects to the given server address
    fn connect(server_session_url: &str, config: &SocketConfig) -> (PacketSender, PacketReceiver) {
        return Socket::connect(server_session_url, config);
    }
}
