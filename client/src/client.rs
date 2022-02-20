use std::{collections::VecDeque, hash::Hash, marker::PhantomData, net::SocketAddr};

use naia_client_socket::{Packet, Socket};
use naia_shared::MonitorConfig;
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SharedConfig, SocketConfig, StandardHeader, Timer, Timestamp,
    WorldMutType, WorldRefType,
};

use super::{
    client_config::ClientConfig,
    connection::Connection,
    entity_action::EntityAction,
    entity_ref::{EntityMut, EntityRef},
    error::NaiaClientError,
    event::Event,
    handshake_manager::HandshakeManager,
    io::Io,
    tick_manager::TickManager,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
pub struct Client<P: Protocolize, E: Copy + Eq + Hash> {
    // Config
    client_config: ClientConfig,
    shared_config: SharedConfig<P>,
    connection_config: ConnectionConfig,
    socket_config: SocketConfig,
    monitor_config: Option<MonitorConfig>,
    // Connection
    io: Io,
    server_connection: Option<Connection<P, E>>,
    handshake_manager: HandshakeManager<P>,
    // Events
    outstanding_connect: bool,
    outstanding_disconnect: bool,
    outstanding_errors: VecDeque<NaiaClientError>,
    // Ticks
    tick_manager: Option<TickManager>,
    // Phantom
    phantom_k: PhantomData<E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Client<P, E> {
    /// Create a new Client
    pub fn new(client_config: ClientConfig, shared_config: SharedConfig<P>) -> Self {
        let connection_config = client_config.connection_config.clone();
        let monitor_config = shared_config.monitor_config.clone();

        let mut socket_config = client_config.socket_config.clone();
        socket_config.link_condition_config = shared_config.link_condition_config.clone();

        let handshake_manager = HandshakeManager::new(client_config.send_handshake_interval);

        let tick_manager = {
            if let Some(duration) = shared_config.tick_interval {
                Some(TickManager::new(duration, client_config.minimum_latency))
            } else {
                None
            }
        };

        Client {
            // Config
            client_config,
            shared_config,
            connection_config,
            socket_config,
            monitor_config,
            // Connection
            io: Io::new(),
            server_connection: None,
            handshake_manager,
            // Events
            outstanding_connect: false,
            outstanding_disconnect: false,
            outstanding_errors: VecDeque::new(),
            // Ticks
            tick_manager,
            // Phantom
            phantom_k: PhantomData,
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<R: ReplicateSafe<P>>(&mut self, auth: R) {
        if !self.is_disconnected() {
            panic!("Must call client.auth(..) BEFORE calling client.connect(..)");
        }
        self.handshake_manager
            .set_auth_message(auth.into_protocol());
    }

    /// Connect to the given server address
    pub fn connect(&mut self, server_session_url: &str) {
        if !self.is_disconnected() {
            panic!("Client has already initiated a connection, cannot initiate a new one. TIP: Check client.is_disconnected() before calling client.connect()");
        }
        let mut socket = Socket::new(self.socket_config.clone());
        socket.connect(server_session_url);
        self.io
            .load(socket.packet_sender(), socket.packet_receiver());
    }

    /// Returns whether or not the client is disconnected
    pub fn is_disconnected(&self) -> bool {
        !self.io.is_loaded()
    }

    /// Returns whether or not a connection is being established with the Server
    pub fn is_connecting(&self) -> bool {
        self.io.is_loaded()
    }

    /// Returns whether or not a connection has been established with the Server
    pub fn is_connected(&self) -> bool {
        self.server_connection.is_some()
    }

    /// Disconnect from Server
    pub fn disconnect(&mut self) {
        if !self.is_connected() {
            panic!("Trying to disconnect Client which is not connected yet!")
        }

        // get current tick
        let client_tick_opt = self.client_tick();

        if let Some(connection) = &mut self.server_connection {
            let disconnect_packet = self.handshake_manager.disconnect_packet();
            for _ in 0..10 {
                internal_send_with_connection::<P, E>(
                    client_tick_opt,
                    &mut self.io,
                    connection,
                    PacketType::Disconnect,
                    disconnect_packet.clone(),
                );
            }
            self.outstanding_disconnect = true;
        }
    }

    // Messages

    /// Queues up an Message to be sent to the Server
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .message_manager
                .send_message(message, guaranteed_delivery);
        }
    }

    // Entities

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<'s, W: WorldRefType<P, E>>(&'s self, world: W, entity: &E) -> EntityRef<P, E, W> {
        return EntityRef::new(world, &entity);
    }

    /// Retrieves an EntityMut that exposes write operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<'s>(&'s mut self, entity: &E) -> EntityMut<P, E> {
        return EntityMut::new(self, &entity);
    }

    /// Return a list of all Entities
    pub fn entities<W: WorldRefType<P, E>>(&self, world: &W) -> Vec<E> {
        return world.entities();
    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        return self.io.server_addr_unwrapped();
    }

    /// Gets the network

    /// Gets the average Round Trip Time measured to the Server
    pub fn rtt(&self) -> f32 {
        return self
            .server_connection
            .as_ref()
            .unwrap()
            .ping_manager
            .as_ref()
            .expect("SharedConfig.monitor_config is set to None! Enable to allow checking RTT.")
            .rtt;
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn jitter(&self) -> f32 {
        return self
            .server_connection
            .as_ref()
            .unwrap()
            .ping_manager
            .as_ref()
            .expect("SharedConfig.monitor_config is set to None! Enable to allow checking RTT.")
            .jitter;
    }

    // Ticks

    /// Gets the current tick of the Client
    pub fn client_tick(&self) -> Option<u16> {
        return self
            .tick_manager
            .as_ref()
            .map(|tick_manager| tick_manager.client_sending_tick());
    }

    // Interpolation

    /// Gets the interpolation tween amount for the current frame
    pub fn interpolation(&self) -> Option<f32> {
        self.tick_manager
            .as_ref()
            .map(|tick_manager| tick_manager.interpolation())
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

        // send ticks, handshakes, heartbeats, pings, timeout if need be
        if self.server_connection.is_some() {
            let mut did_tick = false;

            // update current tick
            if let Some(tick_manager) = &mut self.tick_manager {
                if tick_manager.receive_tick() {
                    did_tick = true;

                    // apply updates on tick boundary
                    let receiving_tick = tick_manager.client_receiving_tick();
                    self.server_connection
                        .as_mut()
                        .unwrap()
                        .process_buffered_packets(
                            &mut world,
                            &self.shared_config.manifest,
                            receiving_tick,
                        );
                    self.server_connection
                        .as_mut()
                        .unwrap()
                        .entity_manager
                        .message_sender
                        .on_tick(tick_manager.server_receivable_tick());
                }
            } else {
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .process_buffered_packets(&mut world, &self.shared_config.manifest, 0);
            }
            // return connect event
            if self.outstanding_connect {
                let server_addr = self.server_address_unwrapped_mut();
                events.push_back(Ok(Event::Connection(server_addr)));
                self.outstanding_connect = false;
            }
            // new errors
            while let Some(err) = self.outstanding_errors.pop_front() {
                events.push_back(Err(err));
            }
            // drop connection if necessary
            if self.server_connection.as_ref().unwrap().base.should_drop()
                || self.outstanding_disconnect
            {
                let server_addr = self.server_address_unwrapped();
                self.disconnect_cleanup();
                events.clear();
                events.push_back(Ok(Event::Disconnection(server_addr)));

                // exit early, we're disconnected, who cares?
                return events;
            }
            // receive messages
            while let Some(message) = self
                .server_connection
                .as_mut()
                .unwrap()
                .base
                .message_manager
                .pop_incoming_message()
            {
                events.push_back(Ok(Event::Message(message)));
            }
            // receive entity actions
            while let Some(action) = self
                .server_connection
                .as_mut()
                .unwrap()
                .entity_manager
                .incoming_entity_action()
            {
                let event: Event<P, E> = {
                    match action {
                        EntityAction::SpawnEntity(entity, component_list) => Event::SpawnEntity(entity, component_list),
                        EntityAction::DespawnEntity(entity) => Event::DespawnEntity(entity),
                        EntityAction::MessageEntity(entity, message) => Event::MessageEntity(entity, message.clone()),
                        EntityAction::InsertComponent(entity, component_key) => Event::InsertComponent(entity, component_key),
                        EntityAction::UpdateComponent(tick, entity, component_key) => Event::UpdateComponent(tick, entity, component_key),
                        EntityAction::RemoveComponent(entity, component) => Event::RemoveComponent(entity, component.clone()),
                    }
                };
                events.push_back(Ok(event));
            }

            // send outgoing packets
            let client_tick = self.client_tick().unwrap_or(0);
            let mut sent = false;
            while let Some(payload) = self
                .server_connection
                .as_mut()
                .unwrap()
                .outgoing_packet(client_tick)
            {
                self.io.send_packet(Packet::new_raw(payload));
                sent = true;
            }
            if sent {
                self.server_connection.as_mut().unwrap().base.mark_sent();
            }

            // tick event
            if did_tick {
                events.push_back(Ok(Event::Tick));
            }
        } else {
            self.handshake_manager.send_packet(&mut self.io);
        }

        events
    }

    // Crate-public functions

    /// Sends a Message to the Server, associated with a given Entity
    pub(crate) fn send_entity_message<R: ReplicateSafe<P>>(&mut self, entity: &E, message: &R) {
        if let Some(client_tick) = self.client_tick() {
            if let Some(connection) = self.server_connection.as_mut() {
                connection
                    .entity_manager
                    .message_sender
                    .send_entity_message(entity, message, client_tick);
            }
        }
    }

    // internal functions

    fn maintain_socket(&mut self) {

        // get current tick
        let client_tick_opt = self.client_tick();

        // send heartbeats
        if self
            .server_connection
            .as_ref()
            .unwrap()
            .base
            .should_send_heartbeat()
        {
            internal_send_with_connection::<P, E>(
                client_tick_opt,
                &mut self.io,
                self.server_connection.as_mut().unwrap(),
                PacketType::Heartbeat,
                Packet::empty(),
            );
        }
        // send pings
        if let Some(ping_manager) = &mut self.server_connection.as_mut().unwrap().ping_manager {
            if ping_manager.should_send_ping() {
                let ping_packet = ping_manager.ping_packet();
                internal_send_with_connection::<P, E>(
                    client_tick_opt,
                    &mut self.io,
                    self.server_connection.as_mut().unwrap(),
                    PacketType::Ping,
                    ping_packet,
                );
            }
        }

        // receive from socket
        loop {
            match self.io.receive_packet() {
                Ok(event) => {
                    if let Some(packet) = event {
                        let server_connection_wrapper = self.server_connection.as_mut();

                        if let Some(server_connection) = server_connection_wrapper {
                            server_connection.base.mark_heard();

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
                                    server_connection
                                        .buffer_data_packet(header.host_tick(), &payload);
                                }
                                PacketType::Heartbeat => {}
                                PacketType::Pong => {
                                    if let Some(ping_manager) = &mut server_connection.ping_manager
                                    {
                                        ping_manager.process_pong(&payload);
                                    }
                                }
                                // TODO: explicitly cover these cases
                                _ => {}
                            }
                        } else {
                            self.handshake_manager.receive_packet(packet);
                            if self.handshake_manager.is_connected() {
                                let server_connection = Connection::new(
                                    self.server_address_unwrapped(),
                                    &self.connection_config,
                                    &self.monitor_config,
                                );

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

    fn disconnect_cleanup(&mut self) {
        // this is very similar to the newtype method .. can we coalesce and reduce
        // duplication?
        let handshake_manager = HandshakeManager::new(self.client_config.send_handshake_interval);
        let tick_manager = {
            if let Some(duration) = self.shared_config.tick_interval {
                Some(TickManager::new(
                    duration,
                    self.client_config.minimum_latency,
                ))
            } else {
                None
            }
        };

        self.io = Io::new();
        self.server_connection = None;
        self.handshake_manager = handshake_manager;
        self.outstanding_connect = false;
        self.outstanding_disconnect = false;
        self.outstanding_errors = VecDeque::new();
        self.tick_manager = tick_manager;
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        return self.io.server_addr_unwrapped();
    }

    fn server_address_unwrapped_mut(&mut self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        return self.io.server_addr_unwrapped();
    }
}

fn internal_send_with_connection<P: Protocolize, E: Copy + Eq + Hash>(
    client_tick: Option<u16>,
    io: &mut Io,
    connection: &mut Connection<P, E>,
    packet_type: PacketType,
    packet: Packet,
) {
    let new_payload = connection.base.process_outgoing_header(
        client_tick.unwrap_or(0),
        packet_type,
        packet.payload(),
    );
    io.send_packet(Packet::new_raw(new_payload));
    connection.base.mark_sent();
}
