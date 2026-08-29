use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    panic,
    time::Duration,
};

use log::{info, warn};

use naia_shared::{
    BigMap, BitReader, BitWriter, DisconnectReason, FakeEntityConverter, Message, MessageContainer,
    MessageKinds, PacketType, Protocol, ProtocolId, Serde, SocketConfig, StandardHeader,
};

use crate::{
    connection::io::{new_io_pair, RecvIo, SendIo},
    events::main_events::MainEvents,
    handshake::{HandshakeAction, HandshakeManager, Handshaker},
    transport::{AuthReceiver, AuthSender, PacketSender, Socket},
    MainUser, MainUserRef, NaiaServerError, ServerConfig, UserKey,
};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct MainServer {
    // Protocol
    socket_config: SocketConfig,
    message_kinds: MessageKinds,
    // Config
    require_auth: bool,
    pending_auth_timeout: Duration,
    max_pending_auth_users: usize,
    // cont
    recv_io: RecvIo,
    send_io: SendIo,
    auth_io: Option<(Box<dyn AuthSender>, Box<dyn AuthReceiver>)>,
    handshake_manager: Box<dyn Handshaker>,
    // Users
    users: BigMap<UserKey, MainUser>,
    user_connections: HashMap<SocketAddr, UserKey>,
    /// Users that have sent an auth request but have not yet completed the
    /// handshake. Tracked explicitly so the capacity check and the
    /// pending-auth timeout sweep are both proportional to the pending set
    /// rather than to every connected user.
    pending_auth_users: HashSet<UserKey>,
    // Events
    incoming_events: MainEvents,
}

impl MainServer {
    /// Create a new MainServer
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let mut protocol: Protocol = protocol.into();
        protocol.lock();
        let protocol_id = protocol.protocol_id();
        Self::new_with_protocol_id(server_config, protocol, protocol_id)
    }

    /// Creates a new `MainServer` using a pre-computed protocol ID (used by adapters sharing a protocol).
    pub fn new_with_protocol_id(
        server_config: ServerConfig,
        protocol: Protocol,
        protocol_id: ProtocolId,
    ) -> Self {
        let Protocol {
            socket,
            message_kinds,
            compression,
            ..
        } = protocol;

        let (recv_io, send_io) = new_io_pair(
            &server_config.connection.bandwidth_measure_duration,
            &compression,
        );

        Self {
            // Config
            socket_config: socket,
            message_kinds,
            require_auth: server_config.require_auth,
            pending_auth_timeout: server_config.pending_auth_timeout,
            max_pending_auth_users: server_config.max_pending_auth_users,
            // Connection
            recv_io,
            send_io,
            auth_io: None,
            handshake_manager: Box::new(HandshakeManager::new(protocol_id)),
            // Users
            users: BigMap::new(),
            user_connections: HashMap::new(),
            pending_auth_users: HashSet::new(),
            // Events
            incoming_events: MainEvents::default(),
        }
    }

    /// Listen at the given addresses
    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        let boxed_socket: Box<dyn Socket> = socket.into();
        let (auth_sender, auth_receiver, packet_sender, packet_receiver) = boxed_socket.listen();

        self.recv_io.load(packet_receiver);
        self.send_io.load(packet_sender);

        self.auth_io = Some((auth_sender, auth_receiver));
    }

    /// Returns a cloned handle to the underlying packet sender.
    pub fn sender_cloned(&self) -> Box<dyn PacketSender> {
        self.send_io.sender_cloned()
    }

    /// Resets all handshake state, user connections, and pending events back to defaults.
    pub fn reset_all(&mut self) {
        self.handshake_manager.reset();
        self.users = BigMap::new();
        self.user_connections.clear();
        self.incoming_events = MainEvents::default();
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.send_io.is_loaded()
    }

    /// Returns socket config
    pub fn socket_config(&self) -> &SocketConfig {
        &self.socket_config
    }

    /// Must be called regularly, maintains connection to and receives messages
    /// from all Clients
    pub fn receive(&mut self) -> MainEvents {
        // Need to run this to maintain connection with all clients, and receive packets
        // until none left
        self.maintain_socket();

        // return all received messages and reset the buffer
        std::mem::take(&mut self.incoming_events)
    }

    // Connections

    /// Accepts an incoming Client User, allowing them to establish a connection
    /// with the Server
    pub fn accept_connection(&mut self, user_key: &UserKey) {
        let Some(user) = self.users.get_mut(user_key) else {
            warn!("unknown user is finalizing connection...");
            return;
        };
        let Some(auth_addr) = user.take_auth_address() else {
            warn!(
                "accept_connection called for a user whose auth request was already \
                 answered -- accept_connection/reject_connection may each be called \
                 at most once per AuthEvent. Ignoring."
            );
            return;
        };

        // info!("adding authenticated user {}", &auth_addr);
        let identity_token = naia_shared::IdentityToken::generate();
        self.handshake_manager
            .authenticate_user(&identity_token, user_key);

        let (auth_sender, _) = self
            .auth_io
            .as_mut()
            .expect("Auth should be set up by this point");
        if auth_sender.accept(&auth_addr, &identity_token).is_err() {
            warn!(
                "Server Error: Cannot send auth accept packet to {:?}",
                &auth_addr
            );
            // TODO: handle destroying any threads waiting on this response
        }
    }

    /// Rejects an incoming Client User, terminating their attempt to establish
    /// a connection with the Server
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        self.reject_connection_with_payload(user_key, None);
    }

    /// Rejects an incoming Client User, handing them a `message` explaining why
    /// (naia-lib/naia#133).
    ///
    /// The message is serialized here, against this server's protocol, and the
    /// client decodes it against its own. Entity properties cannot be resolved
    /// before a connection exists, so a rejection message must not contain any.
    pub fn reject_connection_with<M: Message>(&mut self, user_key: &UserKey, message: M) {
        let container = MessageContainer::new(Box::new(message));
        let mut writer = BitWriter::new();
        container.write(&self.message_kinds, &mut writer, &mut FakeEntityConverter);
        self.reject_connection_with_payload(user_key, Some(writer.to_bytes().to_vec()));
    }

    /// Rejects an incoming Client User, optionally handing them an
    /// already-serialized message explaining why (naia-lib/naia#133).
    pub fn reject_connection_with_payload(&mut self, user_key: &UserKey, payload: Option<Vec<u8>>) {
        if let Some(user) = self.users.get_mut(user_key) {
            let Some(auth_addr) = user.take_auth_address() else {
                warn!(
                    "reject_connection called for a user whose auth request was already \
                     answered -- accept_connection/reject_connection may each be called \
                     at most once per AuthEvent. Ignoring."
                );
                return;
            };

            // info!("rejecting authenticated user {:?}", &auth_addr);
            let (auth_sender, _) = self
                .auth_io
                .as_mut()
                .expect("Auth should be set up by this point");
            if auth_sender.reject(&auth_addr, payload.as_deref()).is_err() {
                warn!(
                    "Server Error: Cannot send auth reject message to {:?}",
                    &auth_addr
                );
                // TODO: handle destroying any threads waiting on this response
            }

            self.user_delete(user_key);
        }
    }

    fn finalize_connection(&mut self, user_key: &UserKey, user_address: &SocketAddr) {
        let Some(user) = self.users.get_mut(user_key) else {
            warn!("unknown user is finalizing connection...");
            return;
        };
        user.set_address(user_address);

        self.user_connections.insert(user.address(), *user_key);
        self.pending_auth_users.remove(user_key);

        self.incoming_events.push_connection(user_key);
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.users.contains_key(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    ///
    /// # Panics
    /// Panics if no user exists for the given key. Prefer [`user_opt`](Self::user_opt)
    /// when the key may be stale.
    pub fn user(&'_ self, user_key: &UserKey) -> MainUserRef<'_> {
        if self.users.contains_key(user_key) {
            return MainUserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns `Some(MainUserRef)` if the user exists, or `None` if the key is stale.
    pub fn user_opt(&'_ self, user_key: &UserKey) -> Option<MainUserRef<'_>> {
        if self.users.contains_key(user_key) {
            Some(MainUserRef::new(self, user_key))
        } else {
            None
        }
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, user) in self.users.iter() {
            if !user.has_address() {
                continue;
            }
            if self.user_connections.contains_key(&user.address()) {
                output.push(user_key);
            }
        }

        output
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.users.len()
    }

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        if let Some(user) = self.users.get(user_key) {
            if user.has_address() {
                return Some(user.address());
            }
        }
        None
    }

    /// The registered message kinds, for serializing a message against this
    /// server's protocol.
    pub fn message_kinds(&self) -> &MessageKinds {
        &self.message_kinds
    }

    /// Sends disconnect packets to the user and removes them from all internal state.
    ///
    /// `reason` and `payload` travel with the packet, so a kicked client learns
    /// why it was dropped instead of guessing (naia-lib/naia#10).
    pub fn disconnect_user(
        &mut self,
        user_key: &UserKey,
        reason: DisconnectReason,
        payload: Option<&[u8]>,
    ) {
        // Send disconnect packets to the client before removing them
        // This mirrors the client-initiated disconnect flow
        if let Some(address) = self.user_address(user_key) {
            // Send multiple times for reliability (like client does)
            for _ in 0..10 {
                let disconnect_packet = self.handshake_manager.write_disconnect(reason, payload);
                if self
                    .send_io
                    .send_packet(&address, disconnect_packet)
                    .is_err()
                {
                    log::warn!("Server Error: Cannot send disconnect packet to {}", address);
                    break;
                }
            }
        }
        self.user_delete(user_key);
    }

    pub(crate) fn user_delete(&mut self, user_key: &UserKey) -> MainUser {
        let Some(user) = self.users.remove(user_key) else {
            panic!("Attempting to delete non-existant user!");
        };
        self.pending_auth_users.remove(user_key);

        if let Some(user_addr) = user.address_opt() {
            info!("deleting authenticated user for {}", user.address());
            self.user_connections.remove(&user_addr);
        }

        self.handshake_manager
            .delete_user(user_key, user.address_opt());

        user
    }

    // Private methods

    /// Maintain connection with a client and read all incoming packet data
    fn maintain_socket(&mut self) {
        // receive auth events
        if let Some((auth_sender, auth_receiver)) = self.auth_io.as_mut() {
            loop {
                match auth_receiver.receive() {
                    Ok(Some((auth_addr, auth_bytes))) => {
                        // Refuse to allocate a user record once the pending-auth
                        // backlog is full. Nothing about the sender has been
                        // verified at this point, so without this ceiling a
                        // source-address flood grows memory at the attacker's
                        // packet rate until the timeout sweep catches up.
                        if self.pending_auth_users.len() >= self.max_pending_auth_users {
                            warn!(
                                "pending-auth backlog full ({}); rejecting auth request from {}",
                                self.max_pending_auth_users, auth_addr
                            );
                            let _ = auth_sender.reject(&auth_addr, None);
                            continue;
                        }

                        // create new user
                        let user_key = self.users.insert(MainUser::new(auth_addr));
                        self.pending_auth_users.insert(user_key);

                        if self.require_auth {
                            // convert bytes into auth object and fire ServerAuthEvent
                            let mut reader = BitReader::new(auth_bytes);
                            let Ok(auth_message) =
                                self.message_kinds.read(&mut reader, &FakeEntityConverter)
                            else {
                                warn!("Server Error: cannot read auth message");
                                continue;
                            };
                            self.incoming_events.push_auth(&user_key, auth_message);
                        } else {
                            // auto-accept: no ServerAuthEvent; generate token and send immediately
                            let user = self.users.get_mut(&user_key).expect("user just inserted");
                            let _ = user.take_auth_address(); // consume the auth address
                            let identity_token = naia_shared::IdentityToken::generate();
                            self.handshake_manager
                                .authenticate_user(&identity_token, &user_key);
                            if auth_sender.accept(&auth_addr, &identity_token).is_err() {
                                warn!(
                                    "Server Error: Cannot send auto-accept packet to {:?}",
                                    &auth_addr
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        // No more auths, break loop
                        break;
                    }
                    Err(_) => {
                        self.incoming_events.push_error(NaiaServerError::RecvError);
                    }
                }
            }
        }

        // receive socket events
        loop {
            match self.recv_io.recv_reader() {
                Ok(Some((address, owned_reader))) => {
                    // receive packet
                    let mut reader = owned_reader.borrow();

                    // read header
                    let Ok(header) = StandardHeader::de(&mut reader) else {
                        // Received a malformed packet
                        // TODO: increase suspicion against packet sender
                        continue;
                    };

                    match header.packet_type {
                        PacketType::Data
                        | PacketType::Heartbeat
                        | PacketType::Pong
                        | PacketType::Ping => {
                            if let Some(user_key) = self.user_connections.get(&address) {
                                self.incoming_events.push_world_packet(
                                    *user_key,
                                    address,
                                    owned_reader.take_buffer(),
                                );
                            }
                        }
                        PacketType::Handshake => {
                            match self.handshake_manager.maintain_handshake(
                                &address,
                                &mut reader,
                                self.user_connections.contains_key(&address),
                            ) {
                                Ok(HandshakeAction::ForwardPacket) => {
                                    if let Some(user_key) = self.user_connections.get(&address) {
                                        self.incoming_events.push_world_packet(
                                            *user_key,
                                            address,
                                            owned_reader.take_buffer(),
                                        );
                                    } else {
                                        warn!(
                                            "Server Error: Cannot forward packet to unknown user.."
                                        );
                                    }
                                }
                                Ok(HandshakeAction::DisconnectUser(user_key)) => {
                                    // Verified disconnect request - queue disconnect in world server
                                    // The Server struct will handle queuing it properly
                                    self.incoming_events.push_queued_disconnect(&user_key);
                                }
                                Ok(HandshakeAction::SendPacket(packet)) => {
                                    if self.send_io.send_packet(&address, packet).is_err() {
                                        // Single send failure is not fatal: the client will
                                        // retry the handshake on its own timeout. Persistent
                                        // failures will surface via connection timeout.
                                        warn!("Server Error: Cannot send packet to {}", &address);
                                    }
                                }
                                Ok(HandshakeAction::FinalizeConnection(
                                    user_key,
                                    validate_packet,
                                )) => {
                                    self.finalize_connection(&user_key, &address);
                                    if self.send_io.send_packet(&address, validate_packet).is_err()
                                    {
                                        // Same rationale as SendPacket above: client retries.
                                        warn!(
                                            "Server Error: Cannot send validation packet to {}",
                                            &address
                                        );
                                    }
                                }
                                Ok(HandshakeAction::None) => {}
                                Err(_err) => {
                                    warn!("Server Error: cannot read malformed packet");
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    // No more packets, break loop
                    break;
                }
                Err(error) => {
                    self.incoming_events
                        .push_error(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }

        // Auto-reject connections that completed the network handshake but where the
        // application never called accept_connection/reject_connection within the timeout.
        if self.auth_io.is_some() {
            let timeout = self.pending_auth_timeout;
            // Collect (user_key, auth_addr) pairs for timed-out pending users.
            let timed_out: Vec<(UserKey, Option<SocketAddr>)> = self
                .pending_auth_users
                .iter()
                .filter_map(|key| self.users.get(key).map(|user| (*key, user)))
                .filter(|(_, user)| !user.has_address() && user.created_at.elapsed() > timeout)
                .map(|(key, user)| (key, user.peek_auth_address()))
                .collect();
            for (user_key, auth_addr_opt) in timed_out {
                if let Some(auth_addr) = auth_addr_opt {
                    warn!(
                        "pending-auth timeout for {}: auto-rejecting after {:?}",
                        auth_addr, timeout
                    );
                    if let Some((auth_sender, _)) = self.auth_io.as_mut() {
                        let _ = auth_sender.reject(&auth_addr, None);
                    }
                }
                self.user_delete(&user_key);
            }
        }
    }
}

#[cfg(test)]
mod pending_auth_capacity_tests {
    use std::{
        net::SocketAddr,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
        time::Duration,
    };

    use naia_shared::{IdentityToken, Protocol};

    use crate::{
        transport::{AuthReceiver, AuthSender, PacketReceiver, PacketSender, RecvError, SendError},
        ServerConfig, UserKey,
    };

    use super::MainServer;

    /// Emits `remaining` auth requests, each from a distinct source address,
    /// then reports the queue as drained. Stands in for a source-address flood.
    #[derive(Clone)]
    struct FloodAuthReceiver {
        remaining: u32,
        payload: Vec<u8>,
    }

    impl AuthReceiver for FloodAuthReceiver {
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            let n = self.remaining;
            let addr: SocketAddr = format!(
                "{}.{}.{}.{}:1",
                1 + (n >> 24) % 200,
                (n >> 16) % 256,
                (n >> 8) % 256,
                n % 256
            )
            .parse()
            .unwrap();
            Ok(Some((addr, &self.payload)))
        }
    }

    #[derive(Clone)]
    struct CountingAuthSender {
        rejects: Arc<AtomicU32>,
    }

    impl AuthSender for CountingAuthSender {
        fn accept(&self, _address: &SocketAddr, _token: &IdentityToken) -> Result<(), SendError> {
            Ok(())
        }
        fn reject(&self, _address: &SocketAddr, _payload: Option<&[u8]>) -> Result<(), SendError> {
            self.rejects.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[derive(Clone)]
    struct SilentPacketIo;

    impl PacketReceiver for SilentPacketIo {
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
            Ok(None)
        }
    }

    impl PacketSender for SilentPacketIo {
        fn send(&self, _address: &SocketAddr, _payload: &[u8]) -> Result<(), SendError> {
            Ok(())
        }
    }

    /// Builds a server whose auth channel is already saturated with `count`
    /// queued requests from distinct addresses, carrying an unregistered
    /// payload — the cheapest thing an attacker can send.
    fn server_under_flood(config: ServerConfig, count: u32) -> (MainServer, Arc<AtomicU32>) {
        let protocol = Protocol::builder().build();
        let rejects = Arc::new(AtomicU32::new(0));

        let mut server = MainServer::new(config, protocol);
        server.recv_io.load(Box::new(SilentPacketIo));
        server.send_io.load(Box::new(SilentPacketIo));
        server.auth_io = Some((
            Box::new(CountingAuthSender {
                rejects: rejects.clone(),
            }),
            Box::new(FloodAuthReceiver {
                remaining: count,
                payload: vec![0xff; 8],
            }),
        ));
        (server, rejects)
    }

    fn reload_flood(server: &mut MainServer, count: u32) {
        if let Some((_, receiver)) = server.auth_io.as_mut() {
            *receiver = Box::new(FloodAuthReceiver {
                remaining: count,
                payload: vec![0xff; 8],
            });
        }
    }

    /// Answering the same auth request twice is an application mistake, not a
    /// broken library invariant -- so it is reported and ignored, the way the
    /// unknown-user case beside it already was. It used to unwrap a `None` auth
    /// address and take the server down with a panic naming nothing.
    #[test]
    fn answering_the_same_auth_request_twice_is_ignored() {
        let (mut server, rejects) = server_under_flood(ServerConfig::default(), 1);
        server.maintain_socket();

        let user_key = *server
            .pending_auth_users
            .iter()
            .next()
            .expect("the flood should have created one pending user");

        server.accept_connection(&user_key);
        // Second answer: the auth address is already gone.
        server.accept_connection(&user_key);

        assert_eq!(
            rejects.load(Ordering::Relaxed),
            0,
            "a duplicate answer must not reach the transport",
        );
    }

    /// The reachable second half of the same mistake: rejecting a request that
    /// was already accepted. (Rejecting twice is not a case -- the first
    /// rejection deletes the user record, so the second call never gets past
    /// the lookup.)
    #[test]
    fn rejecting_a_request_that_was_already_accepted_is_ignored() {
        let (mut server, rejects) = server_under_flood(ServerConfig::default(), 1);
        server.maintain_socket();

        let user_key = *server
            .pending_auth_users
            .iter()
            .next()
            .expect("the flood should have created one pending user");

        server.accept_connection(&user_key);
        server.reject_connection(&user_key);

        assert_eq!(
            rejects.load(Ordering::Relaxed),
            0,
            "an accepted request must not also be rejected on the transport",
        );
    }

    /// Before `max_pending_auth_users` existed, this flood allocated one
    /// `MainUser` per spoofed source address — 50,000 of them in a single
    /// tick, none of which the application was ever told about, because the
    /// unregistered payload fails to parse *after* the record is created.
    #[test]
    fn pending_auth_backlog_is_capped() {
        let config = ServerConfig::default();
        let cap = config.max_pending_auth_users;
        let (mut server, rejects) = server_under_flood(config, 50_000);

        server.maintain_socket();

        assert_eq!(
            server.users.len(),
            cap,
            "a source-address flood must not allocate past the pending-auth cap",
        );
        assert_eq!(server.pending_auth_users.len(), cap);
        assert_eq!(
            rejects.load(Ordering::Relaxed),
            50_000 - cap as u32,
            "every request past the cap should be rejected at the door",
        );
    }

    /// Capacity is a live-set bound, not a lifetime quota: users that finish
    /// the handshake stop counting against it.
    #[test]
    fn completing_the_handshake_frees_pending_capacity() {
        let config = ServerConfig::default();
        let cap = config.max_pending_auth_users;
        let (mut server, _) = server_under_flood(config, 50_000);
        server.maintain_socket();
        assert_eq!(server.pending_auth_users.len(), cap);

        // Graduate ten of them out of the pending set.
        let graduating: Vec<UserKey> = server.pending_auth_users.iter().copied().take(10).collect();
        for (i, user_key) in graduating.iter().enumerate() {
            let addr: SocketAddr = format!("10.0.0.{}:2", i).parse().unwrap();
            server.finalize_connection(user_key, &addr);
        }
        assert_eq!(server.pending_auth_users.len(), cap - 10);

        reload_flood(&mut server, 50);
        server.maintain_socket();

        assert_eq!(
            server.pending_auth_users.len(),
            cap,
            "freed slots should be reusable by new auth requests",
        );
        assert_eq!(server.users.len(), cap + 10);
    }

    /// The timeout sweep also frees capacity, and reaches users the
    /// application was never told about.
    #[test]
    fn the_pending_auth_timeout_frees_capacity() {
        let config = ServerConfig {
            pending_auth_timeout: Duration::ZERO,
            ..ServerConfig::default()
        };
        let (mut server, _) = server_under_flood(config, 50_000);

        server.maintain_socket();
        // The sweep at the end of that same call already expired them all.
        assert_eq!(server.users.len(), 0);
        assert_eq!(server.pending_auth_users.len(), 0);

        reload_flood(&mut server, 50_000);
        server.maintain_socket();
        assert_eq!(server.users.len(), 0);
    }
}
