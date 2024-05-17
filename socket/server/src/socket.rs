use std::net::SocketAddr;

use smol::channel;

use naia_socket_shared::{IdentityToken, SocketConfig};

use super::{
    async_socket::Socket as AsyncSocket,
    auth_receiver::{AuthReceiver, AuthReceiverImpl},
    auth_sender::{AuthSender, AuthSenderImpl},
    conditioned_packet_receiver::ConditionedPacketReceiverImpl,
    executor,
    packet_receiver::{PacketReceiver, PacketReceiverImpl},
    packet_sender::{PacketSender, PacketSenderImpl},
    server_addrs::ServerAddrs,
    NaiaServerSocketError,
};

/// Used to send packets from the Server Socket
#[allow(dead_code)]
pub trait SocketTrait {
    fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>);
}

/// Socket is able to send and receive messages from remote Clients
pub struct Socket;

impl Socket {
    /// Listens on the Socket for incoming communication from Clients
    pub fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        let (from_client_receiver, sender_receiver) =
            Self::setup_receiver_loop(server_addrs, config, None, None);

        Self::setup_sender_loop(config, from_client_receiver, sender_receiver)
    }
    /// Listens on the Socket for incoming communication from Clients
    pub fn listen_with_auth(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (
        Box<dyn AuthSender>,
        Box<dyn AuthReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    ) {
        let (from_client_auth_sender, from_client_auth_receiver) = channel::unbounded();
        let (to_session_all_auth_sender, to_session_all_auth_receiver) = channel::unbounded();
        let from_client_auth_sender = Some(from_client_auth_sender);
        let to_session_all_auth_receiver = Some(to_session_all_auth_receiver);

        let (from_client_receiver, sender_receiver) = Self::setup_receiver_loop(
            server_addrs,
            config,
            from_client_auth_sender,
            to_session_all_auth_receiver,
        );

        let (packet_sender, packet_receiver) =
            Self::setup_sender_loop(config, from_client_receiver, sender_receiver);

        // Setup Sender
        let auth_sender_impl = AuthSenderImpl::new(to_session_all_auth_sender);

        let auth_sender: Box<dyn AuthSender> = Box::new(auth_sender_impl);

        // Setup Receiver
        let auth_receiver: Box<dyn AuthReceiver> =
            Box::new(AuthReceiverImpl::new(from_client_auth_receiver));

        (auth_sender, auth_receiver, packet_sender, packet_receiver)
    }

    fn setup_receiver_loop(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
        from_client_auth_sender: Option<
            channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
        >,
        to_session_all_auth_receiver: Option<
            channel::Receiver<(SocketAddr, Option<IdentityToken>)>,
        >,
    ) -> (
        channel::Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
        channel::Receiver<channel::Sender<(SocketAddr, Box<[u8]>)>>,
    ) {
        // Set up receiver loop
        let (from_client_sender, from_client_receiver) = channel::unbounded();
        let (sender_sender, sender_receiver) = channel::unbounded();

        let server_addrs_clone = server_addrs.clone();
        let config_clone = config.clone();

        executor::spawn(async move {
            // Create async socket
            let mut async_socket = AsyncSocket::listen(
                server_addrs_clone,
                config_clone,
                from_client_auth_sender,
                to_session_all_auth_receiver,
            )
            .await;

            sender_sender.send(async_socket.sender()).await.unwrap();
            //TODO: handle result..

            loop {
                let out_message = async_socket.receive().await;
                from_client_sender.send(out_message).await.unwrap();
                //TODO: handle result..
            }
        })
        .detach();

        (from_client_receiver, sender_receiver)
    }

    fn setup_sender_loop(
        config: &SocketConfig,
        from_client_receiver: channel::Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>,
        sender_receiver: channel::Receiver<channel::Sender<(SocketAddr, Box<[u8]>)>>,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        // Set up sender loop
        let (to_client_sender, to_client_receiver) = channel::unbounded();

        executor::spawn(async move {
            // Create async socket
            let async_sender = sender_receiver.recv().await.unwrap();

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

        return (packet_sender, packet_receiver);
    }
}

impl SocketTrait for Socket {
    fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (Box<dyn PacketSender>, Box<dyn PacketReceiver>) {
        return Socket::listen(server_addrs, config);
    }
}
