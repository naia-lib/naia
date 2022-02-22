use std::{collections::VecDeque, hash::Hash, marker::PhantomData, net::SocketAddr};

use naia_client_socket::{Packet, Socket};
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PingConfig, PacketReader, PacketType,
    ProtocolKindType, Protocolize, ReplicateSafe, SharedConfig, SocketConfig, StandardHeader, Tick,
    Timer, Timestamp, WorldMutType, WorldRefType,
};

use super::{
    client_config::ClientConfig,
    connection::Connection,
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
    // Connection
    io: Io,
    server_connection: Option<Connection<P, E>>,
    handshake_manager: HandshakeManager<P>,
    // Events
    incoming_events: VecDeque<Result<Event<P, E>, NaiaClientError>>,
    // Ticks
    tick_manager: Option<TickManager>,
    // Phantom
    phantom_k: PhantomData<E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Client<P, E> {
    /// Create a new Client
    pub fn new(client_config: &ClientConfig, shared_config: &SharedConfig<P>) -> Self {

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
            client_config: client_config.clone(),
            shared_config: shared_config.clone(),
            // Connection
            io: Io::new(&client_config.connection.bandwidth_measure_duration, &shared_config.compression),
            server_connection: None,
            handshake_manager,
            // Events
            incoming_events: VecDeque::new(),
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
        let mut socket = Socket::new(&self.shared_config.socket);
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
        let client_tick = self.client_tick().unwrap_or(0);

        if let Some(connection) = &mut self.server_connection {
            let disconnect_packet = self.handshake_manager.disconnect_packet();
            for _ in 0..10 {
                internal_send_with_connection::<P, E>(
                    client_tick,
                    &mut self.io,
                    connection,
                    PacketType::Disconnect,
                    disconnect_packet.clone(),
                );
            }
        }

        self.disconnect_internal();
    }

    // Receive Data from Server! Very important!

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming update data, and maintains the connection.
    pub fn receive<W: WorldMutType<P, E>>(
        &mut self,
        mut world: W,
    ) -> VecDeque<Result<Event<P, E>, NaiaClientError>> {
        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();

        let client_tick = self.client_tick().unwrap_or(0);

        // drop connection if necessary
        if self.server_connection.is_some() {
            if self.server_connection.as_ref().unwrap().base.should_drop() {
                self.disconnect_internal();
                return std::mem::take(&mut self.incoming_events);
            }
        }

        // all other operations
        if let Some(server_connection) = self.server_connection.as_mut() {
            let mut did_tick = false;

            // update current tick
            if let Some(tick_manager) = &mut self.tick_manager {
                if tick_manager.receive_tick() {
                    did_tick = true;

                    // apply updates on tick boundary
                    let receiving_tick = tick_manager.client_receiving_tick();
                    server_connection.process_buffered_packets(
                        &mut world,
                        &self.shared_config.manifest,
                        receiving_tick,
                        &mut self.incoming_events,
                    );
                    server_connection
                        .entity_manager
                        .message_sender
                        .on_tick(tick_manager.server_receivable_tick());
                }
            } else {
                server_connection.process_buffered_packets(
                    &mut world,
                    &self.shared_config.manifest,
                    0,
                    &mut self.incoming_events,
                );
            }

            // receive messages
            while let Some(message) = server_connection
                .base
                .message_manager
                .pop_incoming_message()
            {
                self.incoming_events.push_back(Ok(Event::Message(message)));
            }

            // send outgoing packets
            let mut sent = false;
            while let Some(payload) = server_connection.outgoing_packet(client_tick) {
                self.io.send_packet(Packet::new_raw(payload));
                sent = true;
            }
            if sent {
                server_connection.base.mark_sent();
            }

            // tick event
            if did_tick {
                self.incoming_events.push_back(Ok(Event::Tick));
            }
        } else {
            self.handshake_manager.send_packet(&mut self.io);
        }

        return std::mem::take(&mut self.incoming_events);
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
            .expect("SharedConfig.ping_config is set to None! Enable to allow checking RTT.")
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
            .expect("SharedConfig.ping_config is set to None! Enable to allow checking Jitter.")
            .jitter;
    }

    // Ticks

    /// Gets the current tick of the Client
    pub fn client_tick(&self) -> Option<Tick> {
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

    // Bandwidth monitoring
    pub fn upload_bandwidth(&mut self) -> f32 {
        return self.io
            .upload_bandwidth();
    }

    pub fn download_bandwidth(&mut self) -> f32 {
        return self.io
            .download_bandwidth();
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
        let client_tick = self.client_tick().unwrap_or(0);

        if let Some(server_connection) = self.server_connection.as_mut() {
            // connection already established

            // send heartbeats
            if server_connection.base.should_send_heartbeat() {
                internal_send_with_connection::<P, E>(
                    client_tick,
                    &mut self.io,
                    server_connection,
                    PacketType::Heartbeat,
                    Packet::empty(),
                );
            }

            // send pings
            if let Some(ping_manager) = &mut server_connection.ping_manager {
                if ping_manager.should_send_ping() {
                    let ping_packet = ping_manager.ping_packet();
                    internal_send_with_connection::<P, E>(
                        client_tick,
                        &mut self.io,
                        server_connection,
                        PacketType::Ping,
                        ping_packet,
                    );
                }
            }

            // receive from socket
            loop {
                match self.io.receive_packet() {
                    Ok(Some(packet)) => {
                        server_connection.base.mark_heard();

                        let (header, payload) = StandardHeader::read(packet.payload());

                        server_connection
                            .process_incoming_header(&header, self.tick_manager.as_mut());

                        match header.packet_type() {
                            PacketType::Data => {
                                server_connection.buffer_data_packet(header.host_tick(), &payload);
                            }
                            PacketType::Heartbeat => {}
                            PacketType::Pong => {
                                if let Some(ping_manager) = &mut server_connection.ping_manager {
                                    ping_manager.process_pong(&payload);
                                }
                            }
                            // TODO: explicitly cover these cases
                            _ => {}
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(error) => {
                        self.incoming_events
                            .push_back(Err(NaiaClientError::Wrapped(Box::new(error))));
                    }
                }
            }
        } else {
            // No connection established yet

            // receive from socket
            loop {
                match self.io.receive_packet() {
                    Ok(Some(packet)) => {
                        if self.handshake_manager.receive_packet(packet) {
                            let server_addr = self.server_address_unwrapped();
                            self.server_connection = Some(Connection::new(
                                server_addr,
                                &self.client_config.connection,
                                &self.shared_config.ping,
                            ));
                            self.incoming_events
                                .push_back(Ok(Event::Connection(server_addr)));
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(error) => {
                        self.incoming_events
                            .push_back(Err(NaiaClientError::Wrapped(Box::new(error))));
                    }
                }
            }
        }
    }

    fn disconnect_internal(&mut self) {
        let server_addr = self.server_address_unwrapped();
        self.disconnect_cleanup();

        // exit early, we're disconnected, who cares?
        self.incoming_events.clear();
        self.incoming_events
            .push_back(Ok(Event::Disconnection(server_addr)));
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

        self.io = Io::new(&self.client_config.connection.bandwidth_measure_duration, &self.shared_config.compression);
        self.server_connection = None;
        self.handshake_manager = handshake_manager;
        self.tick_manager = tick_manager;
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        return self.io.server_addr_unwrapped();
    }
}

fn internal_send_with_connection<P: Protocolize, E: Copy + Eq + Hash>(
    client_tick: Tick,
    io: &mut Io,
    connection: &mut Connection<P, E>,
    packet_type: PacketType,
    packet: Packet,
) {
    let new_payload =
        connection
            .base
            .process_outgoing_header(client_tick, packet_type, packet.payload());
    io.send_packet(Packet::new_raw(new_payload));
    connection.base.mark_sent();
}
