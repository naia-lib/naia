use std::{net::SocketAddr, sync::Arc};

use futures_util::{pin_mut, select, FutureExt};
use smol::channel;

use naia_socket_shared::SocketConfig;

use crate::AuthResponse;

use super::{
    async_socket::Socket as AsyncSocket, auth_receiver::AuthReceiver, auth_sender::AuthSender,
    executor, packet_receiver::PacketReceiver, packet_sender::PacketSender,
    server_addrs::ServerAddrs, shutdown::shutdown_set, shutdown::ShutdownSignal,
    shutdown::ShutdownWait, NaiaServerSocketError,
};

type ClientAuthSender = channel::Sender<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;
type ClientMsgReceiver = channel::Receiver<Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError>>;
type SenderChannelReceiver = channel::Receiver<channel::Sender<(SocketAddr, Box<[u8]>)>>;
type AuthListenResult = (AuthSender, AuthReceiver, PacketSender, PacketReceiver);

/// Socket is able to send and receive messages from remote Clients
pub struct Socket;

impl Socket {
    /// Listens on the Socket for incoming communication from Clients
    ///
    /// `expected_protocol_id` is the server's protocol fingerprint as 32
    /// lowercase hex digits. The session listener refuses any session request
    /// whose fingerprint header does not match it exactly; see
    /// [`PROTOCOL_ID_HEADER`](naia_socket_shared::PROTOCOL_ID_HEADER). It is
    /// required rather than optional because there is no state in which this
    /// socket should serve a peer it has not compared against.
    ///
    /// Dropping both returned halves releases the bound ports: the last
    /// handle drop ends the background tasks, which own the sockets.
    pub fn listen(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
        expected_protocol_id: &str,
    ) -> (PacketSender, PacketReceiver) {
        let (shutdown_signal, mut shutdown_waits) = shutdown_set(3);
        let mut shutdown_waits = shutdown_waits.drain(..);

        let (from_client_receiver, sender_receiver) = Self::setup_receiver_loop(
            server_addrs,
            config,
            None,
            None,
            expected_protocol_id,
            shutdown_waits.next().unwrap(),
            shutdown_waits.next().unwrap(),
        );

        Self::setup_sender_loop(
            config,
            from_client_receiver,
            sender_receiver,
            shutdown_waits.next().unwrap(),
            &shutdown_signal,
        )
    }
    /// Listens on the Socket for incoming communication from Clients
    ///
    /// See [`listen`](Self::listen) for `expected_protocol_id`. Here the
    /// fingerprint comparison happens strictly before the credential is
    /// base64-decoded or handed to the application.
    pub fn listen_with_auth(
        server_addrs: &ServerAddrs,
        config: &SocketConfig,
        expected_protocol_id: &str,
    ) -> AuthListenResult {
        let (from_client_auth_sender, from_client_auth_receiver) = channel::unbounded();
        let (to_session_all_auth_sender, to_session_all_auth_receiver) = channel::unbounded();
        let from_client_auth_sender = Some(from_client_auth_sender);
        let to_session_all_auth_receiver = Some(to_session_all_auth_receiver);

        let (shutdown_signal, mut shutdown_waits) = shutdown_set(3);
        let mut shutdown_waits = shutdown_waits.drain(..);

        let (from_client_receiver, sender_receiver) = Self::setup_receiver_loop(
            server_addrs,
            config,
            from_client_auth_sender,
            to_session_all_auth_receiver,
            expected_protocol_id,
            shutdown_waits.next().unwrap(),
            shutdown_waits.next().unwrap(),
        );

        let (packet_sender, packet_receiver) = Self::setup_sender_loop(
            config,
            from_client_receiver,
            sender_receiver,
            shutdown_waits.next().unwrap(),
            &shutdown_signal,
        );

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
        expected_protocol_id: &str,
        shutdown: ShutdownWait,
        session_shutdown: ShutdownWait,
    ) -> (ClientMsgReceiver, SenderChannelReceiver) {
        // Set up receiver loop
        let (from_client_sender, from_client_receiver) = channel::unbounded();
        let (sender_sender, sender_receiver) = channel::unbounded();

        let server_addrs_clone = server_addrs.clone();
        let config_clone = config.clone();
        let expected_protocol_id = expected_protocol_id.to_string();

        executor::spawn(async move {
            // Create async socket
            let mut async_socket = AsyncSocket::listen(
                server_addrs_clone,
                config_clone,
                from_client_auth_sender,
                to_session_all_auth_receiver,
                expected_protocol_id,
                session_shutdown,
            )
            .await;

            // A closed channel means the owning Socket was dropped: end this
            // task instead of panicking (this runs on a shared executor).
            if sender_sender.send(async_socket.sender()).await.is_err() {
                return;
            }

            // The receive future never observes handle drops on its own, so
            // race it against shutdown: the last handle drop ends this task
            // and releases the webrtc socket (naia-lib/naia#92).
            let shutdown = shutdown.wait().fuse();
            pin_mut!(shutdown);
            loop {
                let receive = async_socket.receive().fuse();
                pin_mut!(receive);
                select! {
                    _ = shutdown => return,
                    out_message = receive => {
                        if from_client_sender.send(out_message).await.is_err() {
                            return;
                        }
                    }
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
        shutdown: ShutdownWait,
        shutdown_signal: &Arc<ShutdownSignal>,
    ) -> (PacketSender, PacketReceiver) {
        // Set up sender loop
        let (to_client_sender, to_client_receiver) = channel::unbounded();

        executor::spawn(async move {
            // A closed channel or the shutdown signal means the owning Socket
            // was dropped: end this task instead of panicking or spinning
            // (this runs on a shared executor).
            let shutdown = shutdown.wait().fuse();
            pin_mut!(shutdown);
            let recv = sender_receiver.recv().fuse();
            pin_mut!(recv);
            let async_sender = select! {
                _ = shutdown => return,
                result = recv => {
                    let Ok(async_sender) = result else {
                        return;
                    };
                    async_sender
                }
            };

            loop {
                let recv = to_client_receiver.recv().fuse();
                pin_mut!(recv);
                select! {
                    _ = shutdown => return,
                    result = recv => {
                        let Ok(msg) = result else {
                            return;
                        };
                        if async_sender.send(msg).await.is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .detach();

        let conditioner_config = config.link_condition.clone();

        // Setup Sender
        let packet_sender = PacketSender::new(to_client_sender).with_shutdown(shutdown_signal);

        // Setup Receiver
        let packet_receiver = PacketReceiver::new(from_client_receiver, &conditioner_config)
            .with_shutdown(shutdown_signal);

        (packet_sender, packet_receiver)
    }
}
