use std::{collections::VecDeque, hash::Hash, marker::PhantomData, net::SocketAddr};

use naia_client_socket::{Packet, Socket};
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SequenceIterator, SharedConfig, StandardHeader, Timer, Timestamp,
    WorldMutType, WorldRefType,
};

use super::{
    client_config::ClientConfig,
    connection::Connection,
    entity_action::EntityAction,
    entity_ref::{EntityRef, EntityMut},
    error::NaiaClientError,
    event::Event,
    handshake_manager::{HandshakeManager, HandshakeResult},
    io::Io,
    tick_manager::TickManager,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
pub struct Client<P: Protocolize, E: Copy + Eq + Hash> {
    // Manifest
    manifest: Manifest<P>,
    // Connection
    connection_config: ConnectionConfig,
    socket: Socket,
    io: Io,
    address: Option<SocketAddr>,
    server_connection: Option<Connection<P, E>>,
    handshake_manager: HandshakeManager<P>,
    // Events
    outstanding_connect: bool,
    outstanding_errors: VecDeque<NaiaClientError>,
    // Ticks
    tick_manager: Option<TickManager>,
    // Phantom
    phantom_k: PhantomData<E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Client<P, E> {
    /// Create a new Client
    pub fn new(mut client_config: ClientConfig, shared_config: SharedConfig<P>) -> Self {
        client_config.socket_config.link_condition_config =
            shared_config.link_condition_config.clone();

        let connection_config = ConnectionConfig::new(
            client_config.disconnection_timeout_duration,
            client_config.heartbeat_interval,
            client_config.ping_interval,
            client_config.rtt_sample_size,
        );

        let socket = Socket::new(client_config.socket_config);

        let handshake_manager = HandshakeManager::new(client_config.send_handshake_interval);

        let tick_manager = {
            if let Some(duration) = shared_config.tick_interval {
                Some(TickManager::new(
                    duration,
                    client_config.minimum_latency,
                ))
            } else {
                None
            }
        };

        Client {
            // Manifest
            manifest: shared_config.manifest,
            // Connection
            io: Io::new(),
            socket,
            connection_config,
            address: None,
            server_connection: None,
            handshake_manager,
            // Events
            outstanding_connect: false,
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager,
            // Phantom
            phantom_k: PhantomData,
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<R: ReplicateSafe<P>>(&mut self, auth: R) {
        self.handshake_manager
            .set_auth_message(auth.into_protocol());
    }

    /// Connect to the given server address
    pub fn connect(&mut self, server_address: SocketAddr) {
        self.address = Some(server_address);
        self.socket.connect(server_address);
        self.io.load(
            self.socket.get_packet_sender(),
            self.socket.get_packet_receiver(),
        );
    }

    /// Returns whether or not a connection has been established with the Server
    pub fn is_connected(&self) -> bool {
        self.io.is_loaded()
    }

    // Messages

    /// Queues up an Message to be sent to the Server
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        if let Some(connection) = &mut self.server_connection {
            connection.send_message(message, guaranteed_delivery);
        }
    }

    // Entities

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<'s, W: WorldRefType<P, E>>(
        &'s self,
        world: W,
        entity: &E,
    ) -> EntityRef<P, E, W> {
        return EntityRef::new(world, &entity);
    }

    /// Retrieves an EntityMut that exposes write operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<'s>(
        &'s mut self,
        entity: &E,
    ) -> EntityMut<P, E> {
        return EntityMut::new(self, &entity);
    }

    /// Return a list of all Entities
    pub fn entities<W: WorldRefType<P, E>>(&self, world: &W) -> Vec<E> {
        return world.entities();
    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        return self
            .address
            .expect("Client has not initiated connection to Server yet!");
    }

    /// Return whether or not a connection has been established with the Server
    pub fn connected(&self) -> bool {
        return self.server_connection.is_some();
    }

    /// Gets the average Round Trip Time measured to the Server
    pub fn rtt(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_rtt();
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn jitter(&self) -> f32 {
        return self.server_connection.as_ref().unwrap().get_jitter();
    }

    // Ticks

    /// Gets the current tick of the Client
    pub fn client_tick(&self) -> Option<u16> {
        return self.tick_manager.as_ref().map(|tick_manager| tick_manager.client_sending_tick());
    }

    // Interpolation

    /// Gets the interpolation tween amount for the current frame
    pub fn interpolation(&self) -> Option<f32> {
        if let Some(tick_manager) = &self.tick_manager {
            Some(tick_manager.fraction)
        } else {
            None
        }
    }

    // Receive Data from Server! Very important!

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming update data, and maintains the connection.
    pub fn receive<W: WorldMutType<P, E>>(
        &mut self,
        mut world: W,
    ) -> VecDeque<Result<Event<P, E>, NaiaClientError>> {
        let mut events = VecDeque::new();

        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();

        // get current tick
        let client_tick_opt = self.client_tick();

        // send ticks, handshakes, heartbeats, pings, timeout if need be
        match &mut self.server_connection {
            Some(connection) => {
                // return connect event
                if self.outstanding_connect {
                    events.push_back(Ok(Event::Connection));
                    self.outstanding_connect = false;
                }
                // new errors
                while let Some(err) = self.outstanding_errors.pop_front() {
                    events.push_back(Err(err));
                }
                // drop connection if necessary
                if connection.should_drop() {
                    self.server_connection = None;
                    self.handshake_manager.disconnect();
                    events.push_back(Ok(Event::Disconnection));
                    return events; // exit early, we're disconnected, who cares?
                }
                // receive messages
                while let Some(message) = connection.get_incoming_message() {
                    events.push_back(Ok(Event::Message(message)));
                }
                // receive entity actions
                while let Some(action) = connection.get_incoming_entity_action() {
                    let event: Event<P, E> = {
                        match action {
                            EntityAction::SpawnEntity(entity, component_list) => {
                                Event::SpawnEntity(entity, component_list)
                            }
                            EntityAction::DespawnEntity(entity) => Event::DespawnEntity(entity),
                            EntityAction::MessageEntity(entity, message) => {
                                Event::MessageEntity(entity, message.clone())
                            }
                            EntityAction::InsertComponent(entity, component_key) => {
                                Event::InsertComponent(entity, component_key)
                            }
                            EntityAction::UpdateComponent(tick, entity, component_key) => {
                                Event::UpdateComponent(tick, entity, component_key)
                            }
                            EntityAction::RemoveComponent(entity, component) => {
                                Event::RemoveComponent(entity, component.clone())
                            }
                        }
                    };
                    events.push_back(Ok(event));
                }
                // send heartbeats
                if connection.should_send_heartbeat() {
                    internal_send_with_connection::<P, E>(
                        client_tick_opt,
                        &mut self.io,
                        connection,
                        PacketType::Heartbeat,
                        Packet::empty(),
                    );
                }
                // send pings
                if connection.should_send_ping() {
                    let ping_payload = connection.get_ping_payload();
                    internal_send_with_connection::<P, E>(
                        client_tick_opt,
                        &mut self.io,
                        connection,
                        PacketType::Ping,
                        ping_payload,
                    );
                }
                // send packets
                if let Some(client_tick) = client_tick_opt {
                    if let Some(tick_manager) = &self.tick_manager {
                        let mut entity_messages = connection.get_entity_messages(tick_manager.server_receivable_tick());
                        while let Some(payload) = connection.get_outgoing_packet(client_tick, &mut entity_messages) {
                            self.io.send_packet(Packet::new_raw(payload));
                            connection.mark_sent();
                        }
                    }
                }
                // update current tick & apply updates on tick boundary
                if let Some(tick_manager) = &mut self.tick_manager {
                    if connection.frame_begin(&mut world, &self.manifest, tick_manager) {
                        events.push_back(Ok(Event::Tick));
                    }
                } else {
                    connection.tickless_read_incoming(&mut world, &self.manifest);
                }
            }
            None => {
                self.handshake_manager.send_packet(&mut self.io);
            }
        }

        events
    }

    // Crate-public functions

    /// Sends a Message to the Server, associated with a given Entity
    pub(crate) fn send_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entity: &E,
        message: &R,
    ) {
        if let Some(client_tick) = self.client_tick() {
            if let Some(connection) = self.server_connection.as_mut() {
                connection.send_entity_message(entity, message, client_tick);
            }
        }
    }

    // internal functions

    fn maintain_socket(&mut self) {
        // receive from socket
        loop {
            match self.io.receive_packet() {
                Ok(event) => {
                    if let Some(packet) = event {
                        let server_connection_wrapper = self.server_connection.as_mut();

                        if let Some(server_connection) = server_connection_wrapper {
                            server_connection.mark_heard();

                            let (header, payload) = StandardHeader::read(packet.payload());
                            let tick_manager: Option<&mut TickManager> = {
                                if let Some(tick_manager) = &mut self.tick_manager {
                                    Some(tick_manager)
                                } else {
                                    None
                                }
                            };
                            server_connection.process_incoming_header(&header, tick_manager);

                            match header.packet_type() {
                                PacketType::Data => {
                                    server_connection.buffer_data_packet(
                                        header.host_tick(),
                                        header.local_packet_index(),
                                        &payload,
                                    );
                                }
                                PacketType::Heartbeat => {}
                                PacketType::Pong => {
                                    server_connection.process_pong(&payload);
                                }
                                _ => {} // TODO: explicitly cover these cases
                            }
                        } else {
                            if self
                                .handshake_manager
                                .receive_packet(&mut self.tick_manager, packet)
                                == HandshakeResult::Connected
                            {
                                let server_connection =
                                    Connection::new(self.server_address(), &self.connection_config);

                                self.server_connection = Some(server_connection);
                                self.outstanding_connect = true;
                            }
                        }
                    } else {
                        break;
                    }
                }
                Err(error) => {
                    self.outstanding_errors
                        .push_back(NaiaClientError::Wrapped(Box::new(error)));
                }
            }
        }
    }
}

fn internal_send_with_connection<P: Protocolize, E: Copy + Eq + Hash>(
    client_tick: Option<u16>,
    io: &mut Io,
    connection: &mut Connection<P, E>,
    packet_type: PacketType,
    packet: Packet,
) {
    let new_payload = connection.process_outgoing_header(
        client_tick.unwrap_or(0),
        connection.get_last_received_tick(),
        packet_type,
        packet.payload(),
    );
    io.send_packet(Packet::new_raw(new_payload));
    connection.mark_sent();
}
