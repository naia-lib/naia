use std::{collections::VecDeque, hash::Hash, marker::PhantomData, net::SocketAddr};

use naia_client_socket::Socket;

pub use naia_shared::{
    serde::{BitReader, BitWriter, Serde},
    ChannelIndex, ConnectionConfig, EntityHandle, EntityHandleConverter, PacketType, PingConfig,
    PingIndex, ProtocolKindType, Protocolize, ReplicateSafe, SharedConfig, SocketConfig,
    StandardHeader, Tick, Timer, Timestamp, WorldMutType, WorldRefType,
};

use crate::{
    connection::{connection::Connection, handshake_manager::HandshakeManager, io::Io},
    protocol::entity_ref::EntityRef,
    tick::tick_manager::TickManager,
};

use super::{client_config::ClientConfig, error::NaiaClientError, event::Event};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
pub struct Client<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    // Config
    client_config: ClientConfig,
    shared_config: SharedConfig<C>,
    // Connection
    io: Io,
    server_connection: Option<Connection<P, E, C>>,
    handshake_manager: HandshakeManager<P>,
    // Events
    incoming_events: VecDeque<Result<Event<P, E, C>, NaiaClientError>>,
    // Ticks
    tick_manager: Option<TickManager>,
    // Phantom
    phantom_k: PhantomData<E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Client<P, E, C> {
    /// Create a new Client
    pub fn new(client_config: &ClientConfig, shared_config: &SharedConfig<C>) -> Self {
        let handshake_manager = HandshakeManager::new(client_config.send_handshake_interval);

        let tick_manager = shared_config
            .tick_interval
            .map(|duration| TickManager::new(duration, client_config.minimum_latency));

        Client {
            // Config
            client_config: client_config.clone(),
            shared_config: shared_config.clone(),
            // Connection
            io: Io::new(
                &client_config.connection.bandwidth_measure_duration,
                &shared_config.compression,
            ),
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

        for _ in 0..10 {
            let mut writer = self.handshake_manager.write_disconnect();
            self.io.send_writer(&mut writer);
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
    ) -> VecDeque<Result<Event<P, E, C>, NaiaClientError>> {
        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();

        // drop connection if necessary
        if self.server_connection.is_some()
            && self.server_connection.as_ref().unwrap().base.should_drop()
        {
            self.disconnect_internal();
            return std::mem::take(&mut self.incoming_events);
        }

        // all other operations
        if let Some(server_connection) = self.server_connection.as_mut() {
            let mut did_tick = false;

            // update current tick
            if let Some(tick_manager) = &mut self.tick_manager {
                if tick_manager.recv_client_tick() {
                    did_tick = true;

                    // apply updates on tick boundary
                    let receiving_tick = tick_manager.client_receiving_tick();
                    server_connection.process_buffered_packets(
                        &mut world,
                        receiving_tick,
                        &mut self.incoming_events,
                    );
                }
            } else {
                server_connection.process_buffered_packets(
                    &mut world,
                    0,
                    &mut self.incoming_events,
                );
            }

            // receive messages
            let messages = server_connection.base.message_manager.receive_messages();
            for (channel, message) in messages {
                self.incoming_events
                    .push_back(Ok(Event::Message(channel, message)));
            }

            // send outgoing packets
            server_connection.send_outgoing_packets(&mut self.io, &self.tick_manager);

            // tick event
            if did_tick {
                self.incoming_events.push_back(Ok(Event::Tick));
            }
        } else {
            self.handshake_manager.send(&mut self.io);
        }

        std::mem::take(&mut self.incoming_events)
    }

    // Messages

    /// Queues up an Message to be sent to the Server
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, channel: C, message: &R) {
        let channel_settings = self.shared_config.channel.channel(&channel);

        if !channel_settings.can_send_to_server() {
            panic!("Cannot send message to Server on this Channel");
        }

        let tick_buffered = channel_settings.tick_buffered();

        if tick_buffered {
            if self.server_connection.is_none() {
                return;
            }
            if let Some(client_tick) = self.client_tick() {
                let connection = self.server_connection.as_mut().unwrap();
                connection.tick_buffer.as_mut().unwrap().send_message(
                    &client_tick,
                    channel,
                    message.protocol_copy(),
                );
            }
        } else if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .message_manager
                .send_message(channel, message.protocol_copy());
        }
    }

    // Entities

    // /// Duplicates an Entity & all of it's Components
    // pub fn duplicate_entity<W: WorldMutType<P, E>>(&self, mut world: W, entity: &E) -> EntityRef<P, E, W> {
    //     let new_entity = world.duplicate_entity(entity);
    //     return EntityRef::new(world, &new_entity);
    // }

    ///// Syncs the state of two entities
    // pub fn mirror_entities<W: WorldMutType<P, E>>(&self, mut world: W, first_entity: &E, second_entity: &E) {
    //     for component_kind in world.component_kinds(&first_entity) {
    //         world.mirror_components(
    //             &first_entity,
    //             &second_entity,
    //             &component_kind,
    //         );
    //     }
    // }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<P, E>>(&self, world: W, entity: &E) -> EntityRef<P, E, W> {
        EntityRef::new(world, entity)
    }

    // /// Retrieves an EntityMut that exposes read and write operations for the
    // /// Entity.
    // /// Panics if the Entity does not exist.
    // pub fn entity_mut<W: WorldMutType<P, E>>(
    //     &mut self,
    //     world: W,
    //     entity: &E,
    // ) -> EntityMut<P, E, W> {
    //     if world.has_entity(entity) {
    //         return EntityMut::new(world, &entity);
    //     }
    //     panic!("No Entity exists for given Key!");
    // }

    /// Return a list of all Entities
    pub fn entities<W: WorldRefType<P, E>>(&self, world: &W) -> Vec<E> {
        world.entities()
    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> SocketAddr {
        self.io.server_addr_unwrapped()
    }

    /// Gets the average Round Trip Time measured to the Server
    pub fn rtt(&self) -> f32 {
        self.server_connection.as_ref().unwrap().ping_manager.rtt
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn jitter(&self) -> f32 {
        self.server_connection.as_ref().unwrap().ping_manager.jitter
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
    pub fn outgoing_bandwidth(&mut self) -> f32 {
        self.io.outgoing_bandwidth()
    }

    pub fn incoming_bandwidth(&mut self) -> f32 {
        self.io.incoming_bandwidth()
    }

    // internal functions

    fn maintain_socket(&mut self) {
        // get current tick
        if let Some(server_connection) = self.server_connection.as_mut() {
            // connection already established

            // send heartbeats
            if server_connection.base.should_send_heartbeat() {
                let mut writer = BitWriter::new();

                // write header
                server_connection
                    .base
                    .write_outgoing_header(PacketType::Heartbeat, &mut writer);

                // write client tick
                if let Some(tick_manager) = self.tick_manager.as_mut() {
                    tick_manager.write_client_tick(&mut writer);
                }

                // send packet
                self.io.send_writer(&mut writer);
                server_connection.base.mark_sent();
            }

            // send pings
            if server_connection.ping_manager.should_send_ping() {
                let mut writer = BitWriter::new();

                // write header
                server_connection
                    .base
                    .write_outgoing_header(PacketType::Ping, &mut writer);

                // write client tick
                if let Some(tick_manager) = self.tick_manager.as_mut() {
                    tick_manager.write_client_tick(&mut writer);
                }

                // write body
                server_connection.ping_manager.write_ping(&mut writer);

                // send packet
                self.io.send_writer(&mut writer);
                server_connection.base.mark_sent();
            }

            // receive from socket
            loop {
                match self.io.recv_reader() {
                    Ok(Some(mut reader)) => {
                        server_connection.base.mark_heard();

                        let header = StandardHeader::de(&mut reader).unwrap();

                        match header.packet_type {
                            PacketType::Data
                            | PacketType::Heartbeat
                            | PacketType::Ping
                            | PacketType::Pong => {
                                // continue, these packet types are allowed when
                                // connection is established
                            }
                            _ => {
                                // short-circuit, do not need to handle other packet types at this
                                // point
                                continue;
                            }
                        }

                        // Read incoming header
                        server_connection.process_incoming_header(&header);

                        // Record incoming tick
                        let mut incoming_tick = 0;

                        if let Some(tick_manager) = self.tick_manager.as_mut() {
                            incoming_tick = tick_manager.read_server_tick(
                                &mut reader,
                                server_connection.ping_manager.rtt,
                                server_connection.ping_manager.jitter,
                            );
                        }

                        // Handle based on PacketType
                        match header.packet_type {
                            PacketType::Data => {
                                server_connection.buffer_data_packet(incoming_tick, &mut reader);
                            }
                            PacketType::Heartbeat => {
                                // already marked as heard, job done
                            }
                            PacketType::Ping => {
                                // read incoming ping index
                                let ping_index = PingIndex::de(&mut reader).unwrap();

                                // write pong payload
                                let mut writer = BitWriter::new();

                                // write header
                                server_connection
                                    .base
                                    .write_outgoing_header(PacketType::Pong, &mut writer);

                                // write server tick
                                if let Some(tick_manager) = self.tick_manager.as_ref() {
                                    tick_manager.write_client_tick(&mut writer);
                                }

                                // write index
                                ping_index.ser(&mut writer);

                                // send packet
                                self.io.send_writer(&mut writer);
                                server_connection.base.mark_sent();
                            }
                            PacketType::Pong => {
                                server_connection.ping_manager.process_pong(&mut reader);
                            }
                            _ => {
                                // no other packet types matter when connection
                                // is established
                            }
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
            if self.io.is_loaded() {
                // receive from socket
                loop {
                    match self.io.recv_reader() {
                        Ok(Some(mut reader)) => {
                            if self.handshake_manager.recv(&mut reader) {
                                // new connect!
                                let server_addr = self.server_address_unwrapped();
                                self.server_connection = Some(Connection::new(
                                    server_addr,
                                    &self.client_config.connection,
                                    &self.shared_config.channel,
                                    &self.shared_config.tick_interval,
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

        self.io = Io::new(
            &self.client_config.connection.bandwidth_measure_duration,
            &self.shared_config.compression,
        );
        self.server_connection = None;
        self.handshake_manager = HandshakeManager::new(self.client_config.send_handshake_interval);
        self.tick_manager = tick_manager;
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        self.io.server_addr_unwrapped()
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> EntityHandleConverter<E>
    for Client<P, E, C>
{
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E {
        let connection = self
            .server_connection
            .as_ref()
            .expect("cannot handle entity properties unless connection is established");
        connection.entity_manager.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &E) -> EntityHandle {
        let connection = self
            .server_connection
            .as_ref()
            .expect("cannot handle entity properties unless connection is established");
        connection.entity_manager.entity_to_handle(entity)
    }
}
