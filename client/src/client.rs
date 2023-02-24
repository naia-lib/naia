use std::{hash::Hash, net::SocketAddr};

use log::warn;

#[cfg(feature = "bevy_support")]
use bevy_ecs::prelude::Resource;

use naia_client_socket::Socket;

pub use naia_shared::{
    BitReader, BitWriter, Channel, ChannelKind, ChannelKinds, ConnectionConfig,
    EntityDoesNotExistError, EntityHandle, EntityHandleConverter, Message, PacketType, Protocol,
    Replicate, Serde, SocketConfig, StandardHeader, Tick, Timer, Timestamp, WorldMutType,
    WorldRefType,
};
use naia_shared::{GameInstant, PingIndex};

use crate::{
    connection::{
        connection::Connection,
        handshake_manager::{HandshakeManager, HandshakeResult},
        io::Io,
    },
    protocol::entity_ref::EntityRef,
};

use super::{client_config::ClientConfig, error::NaiaClientError, events::Events};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
#[cfg_attr(feature = "bevy_support", derive(Resource))]
pub struct Client<E: Copy + Eq + Hash> {
    // Config
    client_config: ClientConfig,
    protocol: Protocol,
    // Connection
    io: Io,
    server_connection: Option<Connection<E>>,
    handshake_manager: HandshakeManager,
    // Events
    incoming_events: Events<E>,
}

impl<E: Copy + Eq + Hash> Client<E> {
    /// Create a new Client
    pub fn new<P: Into<Protocol>>(client_config: ClientConfig, protocol: P) -> Self {
        let mut protocol: Protocol = protocol.into();
        protocol.lock();

        let handshake_manager = HandshakeManager::new(
            client_config.send_handshake_interval,
            client_config.ping_interval,
            client_config.handshake_pings,
        );

        let compression_config = protocol.compression.clone();

        Client {
            // Config
            client_config: client_config.clone(),
            protocol,
            // Connection
            io: Io::new(
                &client_config.connection.bandwidth_measure_duration,
                &compression_config,
            ),
            server_connection: None,
            handshake_manager,
            // Events
            incoming_events: Events::new(),
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<M: Message>(&mut self, auth: M) {
        self.handshake_manager.set_auth_message(Box::new(auth));
    }

    /// Connect to the given server address
    pub fn connect(&mut self, server_session_url: &str) {
        if !self.is_disconnected() {
            panic!("Client has already initiated a connection, cannot initiate a new one. TIP: Check client.is_disconnected() before calling client.connect()");
        }
        let mut socket = Socket::new(&self.protocol.socket);
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
            match self.io.send_writer(&mut writer) {
                Ok(()) => {}
                Err(_) => {
                    // TODO: pass this on and handle above
                    warn!("Client Error: Cannot send disconnect packet to Server");
                }
            }
        }

        self.disconnect_internal();
    }

    // Receive Data from Server! Very important!

    /// Must call this regularly (preferably at the beginning of every draw
    /// frame), in a loop until it returns None.
    /// Retrieves incoming update data from the server, and maintains the connection.
    pub fn receive<W: WorldMutType<E>>(&mut self, mut world: W) -> Events<E> {
        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();

        // all other operations
        if let Some(connection) = self.server_connection.as_mut() {
            if connection.base.should_drop() {
                self.disconnect_internal();
                return std::mem::take(&mut self.incoming_events);
            }

            let (receiving_tick_happened, sending_tick_happened) =
                connection.time_manager.check_ticks();

            if let Some((prev_receiving_tick, current_receiving_tick)) = receiving_tick_happened {
                // apply updates on tick boundary
                connection.process_buffered_packets(
                    &self.protocol,
                    &mut world,
                    &mut self.incoming_events,
                );

                // receive (process) messages
                connection.receive_messages(&mut self.incoming_events);

                let mut index_tick = prev_receiving_tick.wrapping_add(1);
                loop {
                    self.incoming_events.push_server_tick(index_tick);

                    if index_tick == current_receiving_tick {
                        break;
                    }
                    index_tick = index_tick.wrapping_add(1);
                }
            }

            if let Some((prev_sending_tick, current_sending_tick)) = sending_tick_happened {
                // send outgoing packets
                connection.send_outgoing_packets(&self.protocol, &mut self.io);

                let mut index_tick = prev_sending_tick.wrapping_add(1);
                loop {
                    self.incoming_events.push_client_tick(index_tick);

                    if index_tick == current_sending_tick {
                        break;
                    }
                    index_tick = index_tick.wrapping_add(1);
                }
            }
        } else {
            self.handshake_manager
                .send(&self.protocol.message_kinds, &mut self.io);
        }

        std::mem::take(&mut self.incoming_events)
    }

    // Messages

    /// Queues up an Message to be sent to the Server
    pub fn send_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = M::clone_box(message);
        self.send_message_inner(&ChannelKind::of::<C>(), cloned_message);
    }

    fn send_message_inner(&mut self, channel_kind: &ChannelKind, message: Box<dyn Message>) {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);
        if !channel_settings.can_send_to_server() {
            panic!("Cannot send message to Server on this Channel");
        }

        if channel_settings.tick_buffered() {
            panic!("Cannot call `Client.send_message()` on a Tick Buffered Channel, use `Client.send_tick_buffered_message()` instead");
        }

        if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .message_manager
                .send_message(channel_kind, message);
        }
    }

    pub fn send_tick_buffer_message<C: Channel, M: Message>(&mut self, tick: &Tick, message: &M) {
        let cloned_message = M::clone_box(message);
        self.send_tick_buffer_message_inner(tick, &ChannelKind::of::<C>(), cloned_message);
    }

    fn send_tick_buffer_message_inner(
        &mut self,
        tick: &Tick,
        channel_kind: &ChannelKind,
        message: Box<dyn Message>,
    ) {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_server() {
            panic!("Cannot send message to Server on this Channel");
        }

        if !channel_settings.tick_buffered() {
            panic!("Can only use `Client.send_tick_buffer_message()` on a Channel that is configured for it.");
        }

        if let Some(connection) = self.server_connection.as_mut() {
            connection
                .tick_buffer
                .send_message(tick, channel_kind, message);
        }
    }

    // Entities

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&self, world: W, entity: &E) -> EntityRef<E, W> {
        EntityRef::new(world, entity)
    }

    /// Return a list of all Entities
    pub fn entities<W: WorldRefType<E>>(&self, world: &W) -> Vec<E> {
        world.entities()
    }

    // Connection

    /// Get the address currently associated with the Server
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        self.io.server_addr()
    }

    /// Gets the average Round Trip Time measured to the Server
    pub fn rtt(&self) -> f32 {
        self.server_connection
            .as_ref()
            .expect("it is expected that you should verify whether the client is connected before calling this method")
            .time_manager.rtt()
    }

    /// Gets the average Jitter measured in connection to the Server
    pub fn jitter(&self) -> f32 {
        self.server_connection
            .as_ref()
            .expect("it is expected that you should verify whether the client is connected before calling this method")
            .time_manager.jitter()
    }

    // Ticks

    /// Gets the current tick of the Client
    pub fn client_tick(&self) -> Option<Tick> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.client_sending_tick);
        }
        return None;
    }

    /// Gets the current tick of the Server
    pub fn server_tick(&self) -> Option<Tick> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.client_receiving_tick);
        }
        return None;
    }

    // Interpolation

    /// Gets the interpolation tween amount for the current frame, for use by entities on the Client Tick (i.e. predicted)
    pub fn client_interpolation(&self) -> Option<f32> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.client_interpolation());
        }
        return None;
    }

    /// Gets the interpolation tween amount for the current frame, for use by entities on the Server Tick (i.e. authoritative)
    pub fn server_interpolation(&self) -> Option<f32> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.server_interpolation());
        }
        return None;
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
        if let Some(server_connection) = self.server_connection.as_mut() {
            // connection already established

            // send heartbeats
            if server_connection.base.should_send_heartbeat() {
                let mut writer = BitWriter::new();

                // write header
                server_connection
                    .base
                    .write_outgoing_header(PacketType::Heartbeat, &mut writer);

                // send packet
                if self.io.send_writer(&mut writer).is_err() {
                    // TODO: pass this on and handle above
                    warn!("Client Error: Cannot send heartbeat packet to Server");
                }
                server_connection.base.mark_sent();
            }

            // send pings
            if server_connection.time_manager.send_ping(&mut self.io) {
                server_connection.base.mark_sent();
            }

            // receive from socket
            loop {
                match self.io.recv_reader() {
                    Ok(Some(mut reader)) => {
                        server_connection.base.mark_heard();

                        let header = StandardHeader::de(&mut reader)
                            .expect("unable to parse header from incoming packet");

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

                        // read server tick
                        let Ok(server_tick) = Tick::de(&mut reader) else {
                            warn!("unable to parse server_tick from packet");
                            continue;
                        };

                        // read time since last tick
                        let Ok(server_tick_instant) = GameInstant::de(&mut reader) else {
                            warn!("unable to parse server_tick_instant from packet");
                            continue;
                        };

                        server_connection
                            .time_manager
                            .recv_tick_instant(&server_tick, &server_tick_instant);

                        // Handle based on PacketType
                        match header.packet_type {
                            PacketType::Data => {
                                if server_connection
                                    .buffer_data_packet(&server_tick, &mut reader)
                                    .is_err()
                                {
                                    warn!("unable to parse data packet");
                                    continue;
                                }
                            }
                            PacketType::Heartbeat => {
                                // already marked as heard, job done
                            }
                            PacketType::Ping => {
                                // read incoming ping index
                                let ping_index = PingIndex::de(&mut reader)
                                    .expect("unable to parse an index from Ping packet");

                                // write pong payload
                                let mut writer = BitWriter::new();

                                // write header
                                server_connection
                                    .base
                                    .write_outgoing_header(PacketType::Pong, &mut writer);

                                // write index
                                ping_index.ser(&mut writer);

                                // send packet
                                if self.io.send_writer(&mut writer).is_err() {
                                    // TODO: pass this on and handle above
                                    warn!("Client Error: Cannot send pong packet to Server");
                                }
                                server_connection.base.mark_sent();
                            }
                            PacketType::Pong => {
                                if server_connection
                                    .time_manager
                                    .read_pong(&mut reader)
                                    .is_err()
                                {
                                    // TODO: pass this on and handle above
                                    warn!("Client Error: Cannot process pong packet from Server");
                                }
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
                            .push_error(NaiaClientError::Wrapped(Box::new(error)));
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
                            match self.handshake_manager.recv(&mut reader) {
                                Some(HandshakeResult::Connected(time_manager)) => {
                                    // new connect!
                                    let server_addr = self.server_address_unwrapped();
                                    self.server_connection = Some(Connection::new(
                                        server_addr,
                                        &self.client_config.connection,
                                        &self.protocol.channel_kinds,
                                        time_manager,
                                    ));
                                    self.incoming_events.push_connection(&server_addr);
                                }
                                Some(HandshakeResult::Rejected) => {
                                    let server_addr = self.server_address_unwrapped();
                                    self.incoming_events.clear();
                                    self.incoming_events.push_rejection(&server_addr);
                                    self.disconnect_cleanup();
                                    return;
                                }
                                None => {}
                            }
                        }
                        Ok(None) => {
                            break;
                        }
                        Err(error) => {
                            self.incoming_events
                                .push_error(NaiaClientError::Wrapped(Box::new(error)));
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
        self.incoming_events.push_disconnection(&server_addr);
    }

    fn disconnect_cleanup(&mut self) {
        // this is very similar to the newtype method .. can we coalesce and reduce
        // duplication?

        self.io = Io::new(
            &self.client_config.connection.bandwidth_measure_duration,
            &self.protocol.compression,
        );
        self.server_connection = None;
        self.handshake_manager = HandshakeManager::new(
            self.client_config.send_handshake_interval,
            self.client_config.ping_interval,
            self.client_config.handshake_pings,
        );
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        self.io.server_addr().expect("connection not established!")
    }
}

impl<E: Copy + Eq + Hash> EntityHandleConverter<E> for Client<E> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> E {
        let connection = self
            .server_connection
            .as_ref()
            .expect("cannot handle entity properties unless connection is established");
        connection.entity_manager.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        let connection = self
            .server_connection
            .as_ref()
            .expect("cannot handle entity properties unless connection is established");
        connection.entity_manager.entity_to_handle(entity)
    }
}
