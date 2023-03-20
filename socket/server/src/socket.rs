use futures_util::SinkExt;
use smol::channel;

use naia_socket_shared::SocketConfig;

use super::{
    async_socket::Socket as AsyncSocket,
    executor,
    conditioned_packet_receiver::ConditionedPacketReceiverImpl,
    packet_receiver::{PacketReceiverImpl, PacketReceiver},
    packet_sender::PacketSenderImpl,
    server_addrs::ServerAddrs,
    packet_sender::PacketSender,
};

/// Used to send packets from the Server Socket
pub trait SocketTrait {
    fn listen(server_addrs: &ServerAddrs, config: &SocketConfig) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>);
}

/// Socket is able to send and receive messages from remote Clients
pub struct Socket;

impl Socket {
    /// Listens on the Socket for incoming communication from Clients
    pub fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        // Set up receiver loop
        let (from_client_sender, from_client_receiver) = channel::unbounded();
        let (sender_sender, sender_receiver) = channel::bounded(1);

        let server_addrs_clone = server_addrs.clone();
        let config_clone = config.clone();

        executor::spawn(async move {
            // Create async socket
            let mut async_socket = AsyncSocket::listen(server_addrs_clone, config_clone).await;

            sender_sender.send(async_socket.sender()).await.unwrap();
            //TODO: handle result..

            loop {
                let out_message = async_socket.receive().await;
                from_client_sender.send(out_message).await.unwrap();
                //TODO: handle result..
            }
        })
        .detach();

        // Set up sender loop
        let (to_client_sender, to_client_receiver) = channel::unbounded();

        executor::spawn(async move {
            // Create async socket
            let mut async_sender = sender_receiver.recv().await.unwrap();

            loop {
                if let Ok(msg) = to_client_receiver.recv().await {
                    async_sender.send(msg).await.unwrap();
                    //TODO: handle result..
                }
            }
        })
        .detach();

        let conditioner_config = config.link_condition.clone();

        // Setup Sender
        let packet_sender_impl = PacketSenderImpl::new(to_client_sender);

        let packet_sender: Box<dyn PacketSender> = Box::new(packet_sender_impl);

        // Setup Receiver
        let packet_receiver: Box<dyn PacketReceiver> = match &conditioner_config {
            Some(config) => Box::new(ConditionedPacketReceiverImpl::new(
                from_client_receiver,
                config,
            )),
            None => Box::new(PacketReceiverImpl::new(from_client_receiver)),
        };

        return (
            packet_sender,
            packet_receiver,
        );
    }
}

impl SocketTrait for Socket {
    fn listen(server_addrs: &ServerAddrs, config: &SocketConfig) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        return Socket::listen(server_addrs, config);
    }
}
