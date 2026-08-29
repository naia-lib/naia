use std::net::SocketAddr;

use crate::AuthResponse;
use smol::channel;

use naia_socket_shared::SocketConfig;

use super::{
    async_socket::Socket as AsyncSocket, auth_receiver::AuthReceiver, auth_sender::AuthSender,
    executor, packet_receiver::PacketReceiver, packet_sender::PacketSender,
    server_addrs::ServerAddrs, NaiaServerSocketError,
};

type ClientAuthSender = channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;
type ClientMsgReceiver = channel::Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;
type SenderChannelReceiver = channel::Receiver<channel::Sender<(SocketAddr, Box<[u8]>)>>;
type AuthListenResult = (AuthSender, AuthReceiver, PacketSender, PacketReceiver);

/// Socket is able to send and receive messages from remote Clients
pub struct Socket;

impl Socket {
    /// Listens on the Socket for incoming communication from Clients
    pub fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
    ) -> (PacketSender, PacketReceiver) {
        let (from_client_receiver, sender_receiver) =
            Self::setup_receiver_loop(server_addrs, config, None, None);

        Self::setup_sender_loop(config, from_client_receiver, sender_receiver)
    }
    /// Listens on the Socket for incoming communication from Clients
    pub fn listen_with_auth(server_addrs: &ServerAddrs, config: &SocketConfig) -> AuthListenResult {
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
        let auth_sender = AuthSender::new(to_session_all_auth_sender);

        // Setup Receiver
        let auth_receiver = AuthReceiver::new(from_client_auth_receiver);

        (auth_sender, auth_receiver, packet_sender, packet_receiver)
    }

    fn setup_receiver_loop(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
        from_client_auth_sender: Option<ClientAuthSender>,
        to_session_all_auth_receiver: Option<channel::Receiver<(SocketAddr, AuthResponse)>>,
    ) -> (ClientMsgReceiver, SenderChannelReceiver) {
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

            // A closed channel means the owning Socket was dropped: end this
            // task instead of panicking (this runs on a shared executor).
            if sender_sender.send(async_socket.sender()).await.is_err() {
                return;
            }

            loop {
                let out_message = async_socket.receive().await;
                if from_client_sender.send(out_message).await.is_err() {
                    return;
                }
            }
        })
        .detach();

        (from_client_receiver, sender_receiver)
    }

    fn setup_sender_loop(
        config: &SocketConfig,
        from_client_receiver: ClientMsgReceiver,
        sender_receiver: SenderChannelReceiver,
    ) -> (PacketSender, PacketReceiver) {
        // Set up sender loop
        let (to_client_sender, to_client_receiver) = channel::unbounded();

        executor::spawn(async move {
            // Create async socket. A closed channel means the owning Socket
            // was dropped: end this task instead of panicking or spinning
            // (this runs on a shared executor).
            let Ok(async_sender) = sender_receiver.recv().await else {
                return;
            };

            while let Ok(msg) = to_client_receiver.recv().await {
                if async_sender.send(msg).await.is_err() {
                    return;
                }
            }
        })
        .detach();

        let conditioner_config = config.link_condition.clone();

        // Setup Sender
        let packet_sender = PacketSender::new(to_client_sender);

        // Setup Receiver
        let packet_receiver = PacketReceiver::new(from_client_receiver, &conditioner_config);

        (packet_sender, packet_receiver)
    }
}
