use std::{collections::HashMap, net::SocketAddr, panic};

use log::{info, warn};

use naia_shared::{
    BigMap, BitReader, FakeEntityConverter, MessageKinds, PacketType, Protocol, ProtocolId, Serde,
    SocketConfig, StandardHeader,
};

use crate::{
    connection::io::Io,
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
    // cont
    io: Io,
    auth_io: Option<(Box<dyn AuthSender>, Box<dyn AuthReceiver>)>,
    handshake_manager: Box<dyn Handshaker>,
    // Users
    users: BigMap<UserKey, MainUser>,
    user_connections: HashMap<SocketAddr, UserKey>,
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

        let io = Io::new(
            &server_config.connection.bandwidth_measure_duration,
            &compression,
        );

        Self {
            // Config
            socket_config: socket,
            message_kinds,
            // Connection
            io,
            auth_io: None,
            handshake_manager: Box::new(HandshakeManager::new(protocol_id)),
            // Users
            users: BigMap::new(),
            user_connections: HashMap::new(),
            // Events
            incoming_events: MainEvents::default(),
        }
    }

    /// Listen at the given addresses
    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        let boxed_socket: Box<dyn Socket> = socket.into();
        let (auth_sender, auth_receiver, packet_sender, packet_receiver) = boxed_socket.listen();

        self.io.load(packet_sender, packet_receiver);

        self.auth_io = Some((auth_sender, auth_receiver));
    }

    pub fn sender_cloned(&self) -> Box<dyn PacketSender> {
        self.io.sender_cloned()
    }

    pub fn reset_all(&mut self) {
        self.handshake_manager.reset();
        self.users = BigMap::new();
        self.user_connections.clear();
        self.incoming_events = MainEvents::default();
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.io.is_loaded()
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
        let auth_addr = user.take_auth_address();

        // info!("adding authenticated user {}", &auth_addr);
        let identity_token = naia_shared::generate_identity_token();
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
        if let Some(user) = self.users.get_mut(user_key) {
            let auth_addr = user.take_auth_address();

            // info!("rejecting authenticated user {:?}", &auth_addr);
            let (auth_sender, _) = self
                .auth_io
                .as_mut()
                .expect("Auth should be set up by this point");
            if auth_sender.reject(&auth_addr).is_err() {
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

        self.incoming_events.push_connection(user_key);
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.users.contains_key(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&'_ self, user_key: &UserKey) -> MainUserRef<'_> {
        if self.users.contains_key(user_key) {
            return MainUserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
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

    pub fn disconnect_user(&mut self, user_key: &UserKey) {
        // Send disconnect packets to the client before removing them
        // This mirrors the client-initiated disconnect flow
        if let Some(address) = self.user_address(user_key) {
            // Send multiple times for reliability (like client does)
            for _ in 0..10 {
                let disconnect_packet = self.handshake_manager.write_disconnect();
                if self.io.send_packet(&address, disconnect_packet).is_err() {
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
        if let Some((_, auth_receiver)) = self.auth_io.as_mut() {
            loop {
                match auth_receiver.receive() {
                    Ok(Some((auth_addr, auth_bytes))) => {
                        // create new user
                        let user_key = self.users.insert(MainUser::new(auth_addr));

                        // convert bytes into auth object
                        let mut reader = BitReader::new(auth_bytes);
                        let Ok(auth_message) =
                            self.message_kinds.read(&mut reader, &FakeEntityConverter)
                        else {
                            warn!("Server Error: cannot read auth message");
                            continue;
                        };

                        // send out event
                        self.incoming_events.push_auth(&user_key, auth_message);
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
            match self.io.recv_reader() {
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
                                    if self.io.send_packet(&address, packet).is_err() {
                                        // TODO: pass this on and handle above
                                        warn!("Server Error: Cannot send packet to {}", &address);
                                    }
                                }
                                Ok(HandshakeAction::FinalizeConnection(
                                    user_key,
                                    validate_packet,
                                )) => {
                                    self.finalize_connection(&user_key, &address);
                                    if self.io.send_packet(&address, validate_packet).is_err() {
                                        // TODO: pass this on and handle above
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
    }
}
