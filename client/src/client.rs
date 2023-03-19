use std::{hash::Hash, net::SocketAddr};

use log::warn;

#[cfg(feature = "bevy_support")]
use bevy_ecs::prelude::Resource;

pub use naia_shared::{
    BitReader, BitWriter, Channel, ChannelKind, ChannelKinds, ComponentKind, ConnectionConfig,
    EntityConverter, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, EntityRef,
    FakeEntityConverter, GameInstant, Instant, Message, MessageContainer, PacketType, PingIndex,
    Protocol, Replicate, Serde, SocketConfig, StandardHeader, Tick, Timer, Timestamp, WorldMutType,
    WorldRefType,
};

use crate::{
    connection::{
        base_time_manager::BaseTimeManager,
        connection::Connection,
        handshake_manager::{HandshakeManager, HandshakeResult},
        io::Io,
    },
    transport::Socket,
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, global_world_manager::GlobalWorldManager,
    },
};

use super::{client_config::ClientConfig, error::NaiaClientError, events::Events};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
#[cfg_attr(feature = "bevy_support", derive(Resource))]
pub struct Client<E: Copy + Eq + Hash + Send + Sync> {
    // Config
    client_config: ClientConfig,
    protocol: Protocol,
    // Connection
    io: Io,
    server_connection: Option<Connection<E>>,
    handshake_manager: HandshakeManager,
    manual_disconnect: bool,
    // World
    global_world_manager: GlobalWorldManager<E>,
    // Events
    incoming_events: Events<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> Client<E> {
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
            manual_disconnect: false,
            // World
            global_world_manager: GlobalWorldManager::new(),
            // Events
            incoming_events: Events::new(),
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<M: Message>(&mut self, auth: M) {
        self.handshake_manager
            .set_auth_message(MessageContainer::from(Box::new(auth), &FakeEntityConverter));
    }

    /// Connect to the given server address
    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        if !self.is_disconnected() {
            panic!("Client has already initiated a connection, cannot initiate a new one. TIP: Check client.is_disconnected() before calling client.connect()");
        }
        let boxed_socket: Box<dyn Socket> = socket.into();
        let (packet_sender, packet_receiver) = boxed_socket.connect();
        self.io.load(packet_sender, packet_receiver);
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
            let writer = self.handshake_manager.write_disconnect();
            if self.io.send_packet(writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Client Error: Cannot send disconnect packet to Server");
            }
        }

        self.manual_disconnect = true;
    }

    /// Returns socket config
    pub fn socket_config(&self) -> &SocketConfig {
        &self.protocol.socket
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
            if connection.base.should_drop() || self.manual_disconnect {
                self.disconnect_with_events(&mut world);
                return std::mem::take(&mut self.incoming_events);
            }

            let (receiving_tick_happened, sending_tick_happened) =
                connection.time_manager.check_ticks();

            if let Some((prev_receiving_tick, current_receiving_tick)) = receiving_tick_happened {
                // apply updates on tick boundary
                if connection
                    .process_buffered_packets(
                        &self.protocol,
                        &mut world,
                        &mut self.global_world_manager,
                        &mut self.incoming_events,
                    )
                    .is_err()
                {
                    // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                    warn!("Error reading from buffered packet!");
                }

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
                let now = Instant::now();

                connection.send_outgoing_packets(
                    &self.protocol,
                    &now,
                    &mut self.io,
                    &world,
                    &self.global_world_manager,
                );

                // insert tick events in total range
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

    fn send_message_inner(&mut self, channel_kind: &ChannelKind, message_box: Box<dyn Message>) {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);
        if !channel_settings.can_send_to_server() {
            panic!("Cannot send message to Server on this Channel");
        }

        if channel_settings.tick_buffered() {
            panic!("Cannot call `Client.send_message()` on a Tick Buffered Channel, use `Client.send_tick_buffered_message()` instead");
        }

        if let Some(connection) = &mut self.server_connection {
            let converter = EntityConverter::new(
                &self.global_world_manager,
                &connection.base.local_world_manager,
            );
            let message = MessageContainer::from(message_box, &converter);

            if message.has_entity_properties() {
                // collect all entities in the message
                let entities: Vec<E> = message
                    .entities()
                    .iter()
                    .map(|handle| self.global_world_manager.handle_to_entity(handle).unwrap())
                    .collect();

                // check whether all entities are in scope for the connection
                let all_entities_in_scope = {
                    entities.iter().all(|entity| {
                        connection
                            .base
                            .host_world_manager
                            .entity_channel_is_open(entity)
                    })
                };
                if all_entities_in_scope {
                    // All necessary entities are in scope, so send message
                    connection.base.message_manager.send_message(
                        &self.protocol.message_kinds,
                        &converter,
                        channel_kind,
                        message,
                    );
                } else {
                    // Entity hasn't been added to the User Scope yet, or replicated to Client
                    // yet
                    connection.base.host_world_manager.queue_entity_message(
                        entities,
                        channel_kind,
                        message,
                    );
                }
            } else {
                connection.base.message_manager.send_message(
                    &self.protocol.message_kinds,
                    &converter,
                    channel_kind,
                    message,
                );
            }
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
        message_box: Box<dyn Message>,
    ) {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_server() {
            panic!("Cannot send message to Server on this Channel");
        }

        if !channel_settings.tick_buffered() {
            panic!("Can only use `Client.send_tick_buffer_message()` on a Channel that is configured for it.");
        }

        if let Some(connection) = self.server_connection.as_mut() {
            let converter = EntityConverter::new(
                &self.global_world_manager,
                &connection.base.local_world_manager,
            );
            let message = MessageContainer::from(message_box, &converter);
            connection
                .tick_buffer
                .send_message(tick, channel_kind, message);
        }
    }

    // Entities

    pub fn enable_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        self.spawn_entity_inner(&entity);
    }

    pub fn disable_replication(&mut self, entity: &E) {
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(entity);
    }

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&mut self, mut world: W) -> EntityMut<E, W> {
        self.check_client_authoritative_allowed();

        let entity = world.spawn_entity();
        self.spawn_entity_inner(&entity);

        EntityMut::new(self, world, &entity)
    }

    /// Creates a new Entity with a specific id
    pub fn spawn_entity_at(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        self.spawn_entity_inner(entity)
    }

    fn spawn_entity_inner(&mut self, entity: &E) {
        self.global_world_manager.host_spawn_entity(entity);
        if let Some(connection) = &mut self.server_connection {
            let component_kinds = self.global_world_manager.component_kinds(entity).unwrap();
            connection.base.host_world_manager.init_entity(
                &mut connection.base.local_world_manager,
                entity,
                component_kinds,
            );
        }
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&self, world: W, entity: &E) -> EntityRef<E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<W: WorldMutType<E>>(&mut self, world: W, entity: &E) -> EntityMut<E, W> {
        self.check_client_authoritative_allowed();
        if world.has_entity(entity) {
            return EntityMut::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Return a list of all Entities
    pub fn entities<W: WorldRefType<E>>(&self, world: &W) -> Vec<E> {
        world.entities()
    }

    pub fn entity_owner(&self, entity: &E) -> EntityOwner {
        if let Some(owner) = self.global_world_manager.entity_owner(entity) {
            return owner;
        }
        return EntityOwner::Local;
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

    // Crate-Public methods

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, entity: &E) {
        if !world.has_entity(entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // Actually despawn from world
        world.despawn_entity(entity);

        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(entity);
    }

    pub fn despawn_entity_worldless(&mut self, entity: &E) {
        if let Some(connection) = &mut self.server_connection {
            //remove entity from server connection
            connection.base.host_world_manager.despawn_entity(entity);
        }

        // Remove from ECS Record
        self.global_world_manager.host_despawn_entity(entity);
    }

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: Replicate, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        mut component: R,
    ) {
        if !world.has_entity(entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component.kind();

        if world.has_component_of_kind(entity, &component_kind) {
            // Entity already has this Component type yet, update Component

            let Some(mut component_mut) = world.component_mut::<R>(entity) else {
                panic!("Should never happen because we checked for this above");
            };
            component_mut.mirror(&component);
        } else {
            // Entity does not have this Component type yet, initialize Component

            self.insert_component_worldless(entity, &mut component);

            // actually insert component into world
            world.insert_component(entity, component);
        }
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn insert_component_worldless(&mut self, entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        // insert component into server connection
        if let Some(connection) = &mut self.server_connection {
            // insert component into server connection
            if connection.base.host_world_manager.host_has_entity(entity) {
                connection
                    .base
                    .host_world_manager
                    .insert_component(entity, &component_kind);
            }
        }

        // update in world manager
        self.global_world_manager
            .host_insert_component(entity, component);
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: Replicate, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
    ) -> Option<R> {
        // get component key from type
        let component_kind = ComponentKind::of::<R>();

        self.remove_component_worldless(entity, &component_kind);

        // remove from world
        world.remove_component::<R>(entity)
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn remove_component_worldless(&mut self, entity: &E, component_kind: &ComponentKind) {
        // remove component from server connection
        if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .host_world_manager
                .remove_component(entity, &component_kind);
        }

        // cleanup all other loose ends
        self.global_world_manager
            .host_remove_component(entity, &component_kind);
    }

    // Private methods

    fn check_client_authoritative_allowed(&self) {
        if !self.protocol.client_authoritative_entities {
            panic!("Cannot perform this operation: Client Authoritative Entities are not enabled! Enable them in the Protocol, with the `enable_client_authoritative_entities() method, and note that if you do enable them, to make sure you handle all Spawn/Insert/Update events in the Server, as this may be an attack vector.")
        }
    }

    fn maintain_socket(&mut self) {
        if self.server_connection.is_none() {
            self.maintain_handshake();
        } else {
            self.maintain_connection();
        }
    }

    fn maintain_handshake(&mut self) {
        // No connection established yet

        if !self.io.is_loaded() {
            return;
        }

        // receive from socket
        loop {
            match self.io.recv_reader() {
                Ok(Some(mut reader)) => {
                    match self.handshake_manager.recv(&mut reader) {
                        Some(HandshakeResult::Connected(time_manager)) => {
                            // new connect!
                            self.server_connection = Some(Connection::new(
                                &self.client_config.connection,
                                &self.protocol.channel_kinds,
                                time_manager,
                                &self.global_world_manager,
                            ));

                            let server_addr = self.server_address_unwrapped();
                            self.incoming_events.push_connection(&server_addr);
                        }
                        Some(HandshakeResult::Rejected) => {
                            let server_addr = self.server_address_unwrapped();
                            self.incoming_events.clear();
                            self.incoming_events.push_rejection(&server_addr);
                            self.disconnect_reset_connection();
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

    fn maintain_connection(&mut self) {
        // connection already established

        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Should have checked for this above");
        };

        Self::handle_heartbeats(connection, &mut self.io);
        Self::handle_pings(connection, &mut self.io);

        // receive from socket
        loop {
            match self.io.recv_reader() {
                Ok(Some(mut reader)) => {
                    connection.base.mark_heard();

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
                    connection.process_incoming_header(&header);

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

                    connection
                        .time_manager
                        .recv_tick_instant(&server_tick, &server_tick_instant);

                    // Handle based on PacketType
                    match header.packet_type {
                        PacketType::Data => {
                            if connection
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
                            let Ok(ping_index) = BaseTimeManager::read_ping(&mut reader) else {
                                panic!("unable to read ping index");
                            };
                            BaseTimeManager::send_pong(connection, &mut self.io, ping_index);
                        }
                        PacketType::Pong => {
                            if connection.time_manager.read_pong(&mut reader).is_err() {
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
    }

    fn handle_heartbeats(connection: &mut Connection<E>, io: &mut Io) {
        // send heartbeats
        if connection.base.should_send_heartbeat() {
            let mut writer = BitWriter::new();

            // write header
            connection
                .base
                .write_outgoing_header(PacketType::Heartbeat, &mut writer);

            // send packet
            if io.send_packet(writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Client Error: Cannot send heartbeat packet to Server");
            }
            connection.base.mark_sent();
        }
    }

    fn handle_pings(connection: &mut Connection<E>, io: &mut Io) {
        // send pings
        if connection.time_manager.send_ping(io) {
            connection.base.mark_sent();
        }
    }

    fn disconnect_with_events<W: WorldMutType<E>>(&mut self, world: &mut W) {
        let server_addr = self.server_address_unwrapped();

        self.incoming_events.clear();

        self.despawn_all_remote_entities(world);
        self.disconnect_reset_connection();

        self.incoming_events.push_disconnection(&server_addr);
    }

    fn despawn_all_remote_entities<W: WorldMutType<E>>(&mut self, world: &mut W) {
        // this is very similar to the newtype method .. can we coalesce and reduce
        // duplication?

        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Client is already disconnected!");
        };

        let events = connection
            .base
            .despawn_all_remote_entities(&mut self.global_world_manager, world);

        self.incoming_events.receive_entity_events(events);
    }

    fn disconnect_reset_connection(&mut self) {
        self.server_connection = None;

        self.io = Io::new(
            &self.client_config.connection.bandwidth_measure_duration,
            &self.protocol.compression,
        );

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

impl<E: Copy + Eq + Hash + Send + Sync> EntityHandleConverter<E> for Client<E> {
    fn handle_to_entity(&self, entity_handle: &EntityHandle) -> Result<E, EntityDoesNotExistError> {
        self.global_world_manager.handle_to_entity(entity_handle)
    }

    fn entity_to_handle(&self, entity: &E) -> Result<EntityHandle, EntityDoesNotExistError> {
        self.global_world_manager.entity_to_handle(entity)
    }
}
