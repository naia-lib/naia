use std::{any::Any, collections::VecDeque, hash::Hash, net::SocketAddr, time::Duration};

use log::{debug, info, warn};

use naia_shared::{
    AuthorityError, BitWriter, Channel, ChannelKind, ComponentKind, EntityAndGlobalEntityConverter,
    EntityAuthStatus, EntityDoesNotExistError, EntityEvent, FakeEntityConverter, GameInstant,
    GlobalEntity, GlobalEntityMap, GlobalEntitySpawner, GlobalRequestId, GlobalResponseId,
    GlobalWorldManagerType, HostType, Instant, Message, MessageContainer, OwnedLocalEntity,
    PacketType, Protocol, Replicate, ReplicatedComponent, Request, Response, ResponseReceiveKey,
    ResponseSendKey, Serde, SharedGlobalWorldManager, SocketConfig, StandardHeader, Tick,
    WorldMutType, WorldRefType,
};

use super::{
    client_config::ClientConfig, error::NaiaClientError, world_events::WorldEvents,
    JitterBufferType,
};
use crate::{
    connection::{base_time_manager::BaseTimeManager, connection::Connection, io::Io},
    handshake::{HandshakeManager, HandshakeResult, Handshaker},
    tick_events::TickEvents,
    transport::{IdentityReceiverResult, Socket},
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        global_world_manager::GlobalWorldManager,
    },
    ReplicationConfig,
};

/// Client can send/receive messages to/from a server, and has a pool of
/// in-scope entities/components that are synced with the server
pub struct Client<E: Copy + Eq + Hash + Send + Sync> {
    // Config
    client_config: ClientConfig,
    protocol: Protocol,
    // Connection
    auth_message: Option<Vec<u8>>,
    auth_headers: Option<Vec<(String, String)>>,
    io: Io,
    server_connection: Option<Connection>,
    handshake_manager: Box<dyn Handshaker>,
    manual_disconnect: bool,
    waitlist_messages: VecDeque<(ChannelKind, Box<dyn Message>)>,
    // World
    global_world_manager: GlobalWorldManager,
    global_entity_map: GlobalEntityMap<E>,
    // Events
    incoming_world_events: WorldEvents<E>,
    incoming_tick_events: TickEvents,
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

        // Print protocol ID for SetAuthority at startup

        Self {
            // Config
            client_config: client_config.clone(),
            protocol,
            // Connection
            auth_message: None,
            auth_headers: None,
            io: Io::new(
                &client_config.connection.bandwidth_measure_duration,
                &compression_config,
            ),
            server_connection: None,
            handshake_manager: Box::new(handshake_manager),
            manual_disconnect: false,
            waitlist_messages: VecDeque::new(),
            // World
            global_world_manager: GlobalWorldManager::new(),
            global_entity_map: GlobalEntityMap::new(),
            // Events
            incoming_world_events: WorldEvents::new(),
            incoming_tick_events: TickEvents::new(),
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<M: Message>(&mut self, auth: M) {
        // get auth bytes
        let mut bit_writer = BitWriter::new();
        auth.write(
            &self.protocol.message_kinds,
            &mut bit_writer,
            &mut FakeEntityConverter,
        );
        let auth_bytes = bit_writer.to_bytes();
        self.auth_message = Some(auth_bytes.to_vec());
    }

    pub fn auth_headers(&mut self, headers: Vec<(String, String)>) {
        self.auth_headers = Some(headers);
    }

    /// Connect to the given server address
    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        if !self.is_disconnected() {
            panic!("Client has already initiated a connection, cannot initiate a new one. TIP: Check client.is_disconnected() before calling client.connect()");
        }

        if let Some(auth_bytes) = &self.auth_message {
            if let Some(auth_headers) = &self.auth_headers {
                // connect with auth & headers
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) = boxed_socket
                    .connect_with_auth_and_headers(auth_bytes.clone(), auth_headers.clone());
                self.io.load(id_receiver, packet_sender, packet_receiver);
            } else {
                // connect with auth
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) =
                    boxed_socket.connect_with_auth(auth_bytes.clone());
                self.io.load(id_receiver, packet_sender, packet_receiver);
            }
        } else {
            if let Some(auth_headers) = &self.auth_headers {
                // connect with auth headers
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) =
                    boxed_socket.connect_with_auth_headers(auth_headers.clone());
                self.io.load(id_receiver, packet_sender, packet_receiver);
            } else {
                // connect without auth
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) = boxed_socket.connect();
                self.io.load(id_receiver, packet_sender, packet_receiver);
            }
        }
    }

    /// Returns client's current connection status
    pub fn connection_status(&self) -> ConnectionStatus {
        if self.is_connected() {
            if self.is_disconnecting() {
                return ConnectionStatus::Disconnecting;
            } else {
                return ConnectionStatus::Connected;
            }
        } else {
            if self.is_disconnected() {
                return ConnectionStatus::Disconnected;
            }
            if self.is_connecting() {
                return ConnectionStatus::Connecting;
            }
            panic!("Client is in an unknown connection state!");
        }
    }

    /// Returns whether or not a connection is being established with the Server
    fn is_connecting(&self) -> bool {
        self.io.is_loaded()
    }

    /// Returns whether or not a connection has been established with the Server
    fn is_connected(&self) -> bool {
        self.server_connection.is_some()
    }

    /// Returns whether or not the client is disconnecting
    fn is_disconnecting(&self) -> bool {
        if let Some(connection) = &self.server_connection {
            connection.should_drop() || self.manual_disconnect
        } else {
            false
        }
    }

    /// Returns whether or not the client is disconnected
    fn is_disconnected(&self) -> bool {
        !self.io.is_loaded()
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

    pub fn receive_all_packets(&mut self) {
        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();
    }

    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        // all other operations
        if self.is_disconnecting() {
            self.disconnect_with_events(&mut world);
            return;
        }

        let Some(connection) = &mut self.server_connection else {
            return;
        };

        // receive packets, process into events
        let entity_events = connection.process_packets(
            &mut self.global_entity_map,
            &mut self.global_world_manager,
            &self.protocol,
            &mut world,
            &now,
            &mut self.incoming_world_events,
        );

        self.process_entity_events(&mut world, entity_events);
    }

    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        std::mem::take(&mut self.incoming_world_events)
    }

    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        let Some(connection) = &mut self.server_connection else {
            return TickEvents::default();
        };

        let (receiving_tick_happened, sending_tick_happened) =
            connection.time_manager.collect_ticks(&now);

        // If jitter buffer is in bypass mode, process packets immediately regardless of tick
        // Otherwise, only process on tick boundaries
        let should_read_packets = match self.client_config.jitter_buffer {
            JitterBufferType::Bypass => true,
            JitterBufferType::Real => receiving_tick_happened.is_some(),
        };

        if should_read_packets {
            // read packets on tick boundary, de-jittering
            if connection
                .read_buffered_packets(
                    &self.protocol.channel_kinds,
                    &self.protocol.message_kinds,
                    &self.protocol.component_kinds,
                )
                .is_err()
            {
                // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                warn!("Error reading from buffered packet!");
            }
        }

        if let Some((prev_receiving_tick, current_receiving_tick)) = receiving_tick_happened {
            let mut index_tick = prev_receiving_tick.wrapping_add(1);
            loop {
                self.incoming_tick_events.push_server_tick(index_tick);

                if index_tick == current_receiving_tick {
                    break;
                }
                index_tick = index_tick.wrapping_add(1);
            }
        }

        if let Some((prev_sending_tick, current_sending_tick)) = sending_tick_happened {
            // insert tick events in total range
            let mut index_tick = prev_sending_tick.wrapping_add(1);
            loop {
                self.incoming_tick_events.push_client_tick(index_tick);

                if index_tick == current_sending_tick {
                    break;
                }
                index_tick = index_tick.wrapping_add(1);
            }
        }

        std::mem::take(&mut self.incoming_tick_events)
    }

    pub fn send_all_packets<W: WorldRefType<E>>(&mut self, world: W) {
        if let Some(connection) = &mut self.server_connection {
            let now = Instant::now();

            // send packets
            connection.send_packets(
                &self.protocol,
                &now,
                &mut self.io,
                &world,
                &self.global_entity_map,
                &self.global_world_manager,
            );
        } else {
            if self.io.is_loaded() {
                if let Some(outgoing_packet) = self.handshake_manager.send() {
                    if self.io.send_packet(outgoing_packet).is_err() {
                        // TODO: pass this on and handle above
                        warn!("Client Error: Cannot send handshake packet to Server");
                    }
                }
            }
        }
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
            let mut converter = connection
                .base
                .world_manager
                .entity_converter_mut(&self.global_world_manager);
            let message = MessageContainer::from_write(message_box, &mut converter);
            connection.base.message_manager.send_message(
                &self.protocol.message_kinds,
                &mut converter,
                channel_kind,
                message,
            );
        } else {
            self.waitlist_messages
                .push_back((channel_kind.clone(), message_box));
        }
    }

    //
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaClientError> {
        let cloned_request = Q::clone_box(request);
        // let response_type_id = TypeId::of::<Q::Response>();
        let id = self.send_request_inner(&ChannelKind::of::<C>(), cloned_request)?;
        Ok(ResponseReceiveKey::new(id))
    }

    fn send_request_inner(
        &mut self,
        channel_kind: &ChannelKind,
        // response_type_id: TypeId,
        request_box: Box<dyn Message>,
    ) -> Result<GlobalRequestId, NaiaClientError> {
        let channel_settings = self.protocol.channel_kinds.channel(&channel_kind);

        if !channel_settings.can_request_and_respond() {
            std::panic!("Requests can only be sent over Bidirectional, Reliable Channels");
        }

        let Some(connection) = &mut self.server_connection else {
            warn!("currently not connected to server");
            return Err(NaiaClientError::Message(
                "currently not connected to server".to_string(),
            ));
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        let request_id = connection.global_request_manager.create_request_id();
        let message = MessageContainer::from_write(request_box, &mut converter);
        connection.base.message_manager.send_request(
            &self.protocol.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            message,
        );

        return Ok(request_id);
    }

    /// Sends a Response for a given Request. Returns whether or not was successful.
    pub fn send_response<S: Response>(
        &mut self,
        response_key: &ResponseSendKey<S>,
        response: &S,
    ) -> bool {
        let response_id = response_key.response_id();

        let cloned_response = S::clone_box(response);

        self.send_response_inner(&response_id, cloned_response)
    }

    // returns whether was successful
    fn send_response_inner(
        &mut self,
        response_id: &GlobalResponseId,
        response_box: Box<dyn Message>,
    ) -> bool {
        let Some(connection) = &mut self.server_connection else {
            return false;
        };
        let Some((channel_kind, local_response_id)) = connection
            .global_response_manager
            .destroy_response_id(response_id)
        else {
            return false;
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        let response = MessageContainer::from_write(response_box, &mut converter);
        connection.base.message_manager.send_response(
            &self.protocol.message_kinds,
            &mut converter,
            &channel_kind,
            local_response_id,
            response,
        );
        return true;
    }

    /// Check if a response is available for the given request (non-destructive)
    pub fn has_response<S: Response>(&self, response_key: &ResponseReceiveKey<S>) -> bool {
        let Some(connection) = &self.server_connection else {
            return false;
        };
        let request_id = response_key.request_id();
        connection.global_request_manager.has_response(&request_id)
    }

    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<S> {
        let Some(connection) = &mut self.server_connection else {
            return None;
        };
        let request_id = response_key.request_id();
        let Some(container) = connection
            .global_request_manager
            .destroy_request_id(&request_id)
        else {
            return None;
        };
        let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
            .ok()
            .map(|boxed_s| *boxed_s)
            .unwrap();
        return Some(response);
    }
    //

    fn on_connect(&mut self) {
        // send queued messages
        let messages = std::mem::take(&mut self.waitlist_messages);
        for (channel_kind, message_box) in messages {
            self.send_message_inner(&channel_kind, message_box);
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
            let mut converter = connection
                .base
                .world_manager
                .entity_converter_mut(&self.global_world_manager);
            let message = MessageContainer::from_write(message_box, &mut converter);
            connection
                .tick_buffer
                .send_message(tick, channel_kind, message);
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, mut world: W) -> EntityMut<'_, E, W> {
        self.check_client_authoritative_allowed();

        let world_entity = world.spawn_entity();

        self.spawn_entity_inner(&world_entity);

        EntityMut::new(self, world, &world_entity)
    }

    /// Creates a new Entity with a specific id
    fn spawn_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self.global_entity_map.spawn(*world_entity, None);

        self.global_world_manager.host_spawn_entity(&global_entity);

        let Some(connection) = &mut self.server_connection else {
            return;
        };
        let component_kinds = self
            .global_world_manager
            .component_kinds(&global_entity)
            .unwrap();
        connection
            .base
            .world_manager
            .host_init_entity(&global_entity, component_kinds);
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// given Entity.
    /// Panics if the Entity does not exist.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Retrieves an EntityMut that exposes read and write operations for the
    /// Entity.
    /// Panics if the Entity does not exist.
    pub fn entity_mut<W: WorldMutType<E>>(
        &'_ mut self,
        world: W,
        entity: &E,
    ) -> EntityMut<'_, E, W> {
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

    pub(crate) fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        if let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) {
            if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
                return owner;
            }
        }
        return EntityOwner::Local;
    }

    // Replicate options & authority management

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        self.spawn_entity_inner(&entity);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn disable_entity_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(entity);
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        self.check_client_authoritative_allowed();
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .entity_replication_config(&global_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        self.check_client_authoritative_allowed();
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self.global_world_manager.has_entity(&global_entity) {
            panic!("Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Client, before configuring replication.");
        }
        let entity_owner = self
            .global_world_manager
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned = entity_owner.is_server();
        if server_owned {
            panic!("Client cannot configure replication strategy of Server-owned Entities.");
        }
        let client_owned = entity_owner.is_client();
        if !client_owned {
            panic!("Client cannot configure replication strategy of Entities it does not own.");
        }
        let next_config = config;
        let prev_config = self
            .global_world_manager
            .entity_replication_config(&global_entity)
            .unwrap();
        if prev_config == config {
            panic!(
                "Entity replication config is already set to {:?}. Should not set twice.",
                config
            );
        }
        match prev_config {
            ReplicationConfig::Private => {
                match next_config {
                    ReplicationConfig::Private => {
                        panic!("This should not be possible.");
                    }
                    ReplicationConfig::Public => {
                        // private -> public
                        self.publish_entity(&global_entity, true);
                    }
                    ReplicationConfig::Delegated => {
                        // private -> delegated
                        self.publish_entity(&global_entity, true);
                        self.entity_enable_delegation(world, &global_entity, world_entity, true);
                    }
                }
            }
            ReplicationConfig::Public => {
                match next_config {
                    ReplicationConfig::Private => {
                        // public -> private
                        self.unpublish_entity(&global_entity, true);
                    }
                    ReplicationConfig::Public => {
                        panic!("This should not be possible.");
                    }
                    ReplicationConfig::Delegated => {
                        // public -> delegated
                        self.entity_enable_delegation(world, &global_entity, world_entity, true);
                    }
                }
            }
            ReplicationConfig::Delegated => {
                panic!(
                    "Delegated Entities are always ultimately Server-owned. Client cannot modify."
                )
            }
        }
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        self.check_client_authoritative_allowed();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .ok()?;

        self.global_world_manager
            .entity_authority_status(&global_entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_request_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        self.check_client_authoritative_allowed();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // 1. Set local authority status for Entity
        let result = self
            .global_world_manager
            .entity_request_authority(&global_entity);

        if result.is_ok() {
            // 2. Send request to Server via EntityActionEvent system
            let Some(connection) = &mut self.server_connection else {
                return result;
            };

            connection
                .base
                .world_manager
                .remote_send_request_auth(&global_entity);
        }
        result
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        self.check_client_authoritative_allowed();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // 1. Set local authority status for Entity
        let result = self
            .global_world_manager
            .entity_release_authority(&global_entity);
        if result.is_ok() {
            let Some(connection) = &mut self.server_connection else {
                return result;
            };
            connection
                .base
                .world_manager
                .remote_send_release_auth(&global_entity);
        }
        result
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
        let connection = self.server_connection.as_ref()?;
        return Some(connection.time_manager.client_sending_tick);
    }

    /// Gets the current instant of the Client
    pub fn client_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        return Some(connection.time_manager.client_sending_instant);
    }

    /// Gets the current tick of the Server
    pub fn server_tick(&self) -> Option<Tick> {
        let connection = self.server_connection.as_ref()?;
        return Some(connection.time_manager.client_receiving_tick);
    }

    /// Gets the current instant of the Server
    pub fn server_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        return Some(connection.time_manager.client_receiving_instant);
    }

    pub fn tick_to_instant(&self, tick: Tick) -> Option<GameInstant> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.tick_to_instant(tick));
        }
        return None;
    }

    pub fn tick_duration(&self) -> Option<Duration> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.tick_duration());
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

    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            warn!("attempting to despawn entity that has already been despawned?");
            return;
        };
        if !self.global_world_manager.has_entity(&global_entity) {
            warn!("attempting to despawn entity that has already been despawned?");
            return;
        }

        // check whether we have authority to despawn this entity
        if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
            if owner.is_server() {
                let is_delegated = self.global_world_manager.entity_is_delegated(&global_entity);
                if !is_delegated {
                    panic!("attempting to despawn entity that is not yet delegated. Delegation needs some time to be confirmed by the Server, so check that a despawn is possible by calling `commands.entity(..).replication_config(..).is_delegated()` first.");
                }
                if self
                    .global_world_manager
                    .entity_authority_status(&global_entity)
                    != Some(EntityAuthStatus::Granted)
                {
                    panic!("attempting to despawn entity that we do not have authority over");
                }
            }
        } else {
            panic!("attempting to despawn entity that has no owner");
        }

        if let Some(connection) = &mut self.server_connection {
            //remove entity from server connection
            connection.base.world_manager.despawn_entity(&global_entity);
        }

        // Remove from ECS Record
        self.global_world_manager
            .host_despawn_entity(&global_entity);
    }

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        mut component: R,
    ) {
        if !world.has_entity(entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component.kind();

        // Check if client has permission to mutate this entity
        // For client-owned entities: check if this client is the owner
        // For delegated entities: check if client has Granted authority
        // If not, silently ignore the mutation (matches test expectation that updates are ignored)
        if let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(entity) {
            let owner = self.global_world_manager.entity_owner(&global_entity);
            let is_delegated = self.global_world_manager.entity_is_delegated(&global_entity);
            
            let can_mutate = if is_delegated {
                // For delegated entities, check authority status
                self.global_world_manager
                    .entity_authority_status(&global_entity)
                    == Some(EntityAuthStatus::Granted)
            } else if let Some(owner) = owner {
                // For client-owned non-delegated entities, owner can always mutate
                owner.is_client()
            } else {
                // No owner info - cannot mutate
                false
            };
            
            if !can_mutate {
                // Client doesn't have permission - silently ignore the mutation
                return;
            }
        }

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

    // For debugging purposes only
    pub fn component_name(&self, component_kind: &ComponentKind) -> String {
        self.protocol.component_kinds.kind_to_name(component_kind)
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Register component in GlobalDiffHandler FIRST (before inserting into connection)
        // This ensures that when insert_component is called on the connection's world_manager,
        // the component is already registered in GlobalDiffHandler, allowing UserDiffHandler
        // to successfully register it.
        self.global_world_manager.host_insert_component(
            &self.protocol.component_kinds,
            &global_entity,
            component,
        );

        // insert component into server connection
        if let Some(connection) = &mut self.server_connection {
            // insert component into server connection
            if connection
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                connection
                    .base
                    .world_manager
                    .insert_component(&global_entity, &component_kind);
            } else {
                warn!("Attempting to insert component into a non-existent entity in the server connection. This should not happen.");
            }
        } else {
            warn!("Attempting to insert component into a non-existent entity in the server connection. This should not happen.");
        }

        // if entity is delegated, convert over
        if self
            .global_world_manager
            .entity_is_delegated(&global_entity)
        {
            let accessor = self
                .global_world_manager
                .get_entity_auth_accessor(&global_entity);
            component.enable_delegation(&accessor, None)
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: ReplicatedComponent, W: WorldMutType<E>>(
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
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // remove component from server connection
        if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .world_manager
                .remove_component(&global_entity, &component_kind);
        }

        // cleanup all other loose ends
        self.global_world_manager
            .host_remove_component(&global_entity, &component_kind);
    }

    pub(crate) fn publish_entity(&mut self, global_entity: &GlobalEntity, client_is_origin: bool) {
        if client_is_origin {
            // Send PublishEntity action via EntityActionEvent system
            let Some(connection) = &mut self.server_connection else {
                return;
            };
            connection
                .base
                .world_manager
                .send_publish(HostType::Client, global_entity);
        } else {
            if self
                .global_world_manager
                .entity_replication_config(global_entity)
                != Some(ReplicationConfig::Private)
            {
                panic!("Server can only publish Private entities");
            }
        }
        self.global_world_manager.entity_publish(global_entity);
        // don't need to publish the Entity/Component via the World here, because Remote entities work the same whether they are published or not
    }

    pub(crate) fn unpublish_entity(
        &mut self,
        global_entity: &GlobalEntity,
        client_is_origin: bool,
    ) {
        if client_is_origin {
            // Send UnpublishEntity action via EntityActionEvent system
            let Some(connection) = &mut self.server_connection else {
                return;
            };
            connection
                .base
                .world_manager
                .send_unpublish(HostType::Client, global_entity);
        } else {
            if self
                .global_world_manager
                .entity_replication_config(global_entity)
                != Some(ReplicationConfig::Public)
            {
                panic!("Server can only unpublish Public entities");
            }
        }
        self.global_world_manager.entity_unpublish(global_entity);
        // don't need to publish the Entity/Component via the World here, because Remote entities work the same whether they are published or not
    }

    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_is_origin: bool,
    ) {
        // this should happen BEFORE the world entity/component has been translated over to Delegated
        self.global_world_manager
            .entity_register_auth_for_delegation(global_entity);

        if client_is_origin {
            // info!(
            //     "CLIENT: Sending EnableDelegation to server for {:?}",
            //     global_entity
            // );

            // Send EnableDelegationEntity action via EntityActionEvent system
            let Some(connection) = &mut self.server_connection else {
                return;
            };
            connection.base.world_manager.send_enable_delegation(
                HostType::Client,
                true,
                global_entity,
            );
        } else {
            self.entity_complete_delegation(world, global_entity, world_entity);
            for component_kind in world.component_kinds(world_entity) {
                if !self
                    .global_world_manager
                    .entity_has_component(global_entity, &component_kind)
                {
                    self.global_world_manager
                        .remote_insert_component(global_entity, &component_kind);
                }
            }
            self.global_world_manager
                .entity_update_authority(global_entity, EntityAuthStatus::Available);
        }
    }

    fn entity_complete_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
    ) {
        // info!("client.entity_complete_delegation({:?})", global_entity);

        world.entity_enable_delegation(
            &self.protocol.component_kinds,
            &self.global_entity_map,
            &self.global_world_manager,
            world_entity,
        );

        // this should happen AFTER the world entity/component has been translated over to Delegated
        self.global_world_manager
            .entity_enable_delegation(global_entity);
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_is_origin: bool,
    ) {
        info!("client.entity_disable_delegation");
        if client_is_origin {
            panic!("Cannot disable delegation from Client. Server owns all delegated Entities.");
        }

        // Snapshot authority status BEFORE clearing delegation
        let had_granted = self
            .global_world_manager
            .entity_authority_status(global_entity)
            == Some(EntityAuthStatus::Granted);

        // Clear delegation + authority semantics
        self.global_world_manager
            .entity_disable_delegation(global_entity);
        world.entity_disable_delegation(world_entity);

        // Emit AuthLost (AuthReset) if client had Granted authority
        if had_granted {
            self.incoming_world_events.push_auth_reset(*world_entity);
        }

        // Cleanup connection state (despawn from connection's world_manager, but NOT from client world)
        if let Some(connection) = &mut self.server_connection {
            connection.base.world_manager.despawn_entity(global_entity);
        }

        // Note: We do NOT call despawn_entity_worldless here.
        // Disabling delegation clears authority semantics; entity remains alive in the client world.
        // The entity continues normal replication as undelegated.
    }

    pub(crate) fn entity_update_authority(
        &mut self,
        global_entity: &GlobalEntity,
        world_entity: &E,
        new_auth_status: EntityAuthStatus,
    ) {
        let old_auth_status = self
            .global_world_manager
            .entity_authority_status(global_entity)
            .unwrap();

        self.global_world_manager
            .entity_update_authority(global_entity, new_auth_status);

        // Count when authority state is actually mutated
        #[cfg(feature = "e2e_debug")]
        if new_auth_status == EntityAuthStatus::Granted {
            use crate::counters::CLIENT_HANDLE_SET_AUTH;
            use std::sync::atomic::Ordering;
            CLIENT_HANDLE_SET_AUTH.fetch_add(1, Ordering::Relaxed);
        }

        // Update RemoteEntityChannel's internal AuthChannel status (for migrated entities)
        // This ensures the channel's state machine stays in sync with the global tracker
        if let Some(connection) = &mut self.server_connection {
            // Check if entity exists as RemoteEntity
            let channel_status_before = connection
                .base
                .world_manager
                .get_remote_entity_auth_status(global_entity);

            // Only sync if entity exists as RemoteEntity (i.e., migration completed)
            if channel_status_before.is_some() {
                connection
                    .base
                    .world_manager
                    .remote_receive_set_auth(global_entity, new_auth_status);
            } else {
                warn!(
                    "Entity {:?} not yet migrated to RemoteEntity - channel sync skipped",
                    global_entity
                );
            }
        } else {
            debug!("  No server connection - skipping channel sync");
        }

        // info!(
        //     "<-- Received Entity Update Authority message! {:?} -> {:?}",
        //     old_auth_status, new_auth_status
        // );

        // Updated Host Manager
        match (old_auth_status, new_auth_status) {
            // Grant authority (from any state)
            (EntityAuthStatus::Requested, EntityAuthStatus::Granted) |
            (EntityAuthStatus::Denied, EntityAuthStatus::Granted) |
            (EntityAuthStatus::Available, EntityAuthStatus::Granted) => {
                // Register and emit grant event
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .world_manager
                    .register_authed_entity(&self.global_world_manager, global_entity);
                self.incoming_world_events.push_auth_grant(*world_entity);
                #[cfg(feature = "e2e_debug")]
                {
                    use crate::counters::CLIENT_EMIT_AUTH_GRANTED_EVENT;
                    use std::sync::atomic::Ordering;
                    CLIENT_EMIT_AUTH_GRANTED_EVENT.fetch_add(1, Ordering::Relaxed);
                }
            }
            // Lose authority (must deregister and emit reset)
            (EntityAuthStatus::Granted, EntityAuthStatus::Available) |
            (EntityAuthStatus::Granted, EntityAuthStatus::Denied) => {
                // Deregister and emit reset event
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, global_entity);
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            // Request denied (only when Requested -> Denied)
            (EntityAuthStatus::Requested, EntityAuthStatus::Denied) => {
                // Emit denied event, but do NOT deregister (never had authority)
                self.incoming_world_events.push_auth_deny(*world_entity);
            }
            // Release flow
            (EntityAuthStatus::Releasing, EntityAuthStatus::Available) => {
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, global_entity);
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            (EntityAuthStatus::Releasing, EntityAuthStatus::Denied) => {
                // Server takeover during release
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, global_entity);
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            (EntityAuthStatus::Releasing, EntityAuthStatus::Granted) => {
                // Grant arrived during release - treat as Available
                self.global_world_manager
                    .entity_update_authority(global_entity, EntityAuthStatus::Available);
            }
            // Other valid transitions (no side effects)
            (EntityAuthStatus::Available, EntityAuthStatus::Denied) => {
                // Enter scope while someone else holds - no event needed
            }
            (EntityAuthStatus::Denied, EntityAuthStatus::Available) => {
                // Release by someone else - emit reset
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            (EntityAuthStatus::Available, EntityAuthStatus::Available) |
            (EntityAuthStatus::Denied, EntityAuthStatus::Denied) => {
                // Idempotent - no action needed
            }
            (_, _) => {
                panic!(
                    "-- Entity {:?} updated authority, not handled -- {:?} -> {:?}",
                    global_entity, old_auth_status, new_auth_status
                );
            }
        }
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

        if !self.io.is_authenticated() {
            match self.io.recv_auth() {
                IdentityReceiverResult::Success(id_token) => {
                    self.handshake_manager.set_identity_token(id_token);
                }
                IdentityReceiverResult::Waiting => {
                    return;
                }
                IdentityReceiverResult::ErrorResponseCode(code) => {
                    warn!("Authentication error status code: {}", code);

                    let old_socket_addr_result = self.io.server_addr();

                    // reset connection
                    self.io = Io::new(
                        &self.client_config.connection.bandwidth_measure_duration,
                        &self.protocol.compression,
                    );

                    if code == 401 {
                        // push out rejection
                        match old_socket_addr_result {
                            Ok(old_socket_addr) => {
                                self.incoming_world_events.push_rejection(&old_socket_addr);
                            }
                            Err(err) => {
                                self.incoming_world_events.push_error(err);
                            }
                        }
                    } else {
                        // push out error
                        self.incoming_world_events
                            .push_error(NaiaClientError::IdError(code));
                    }

                    return;
                }
            }
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
                                self.client_config.jitter_buffer,
                            ));
                            self.on_connect();

                            let server_addr = self.server_address_unwrapped();
                            self.incoming_world_events.push_connection(&server_addr);
                        }
                        // Some(HandshakeResult::Rejected) => {
                        //     let server_addr = self.server_address_unwrapped();
                        //     self.incoming_events.clear();
                        //     self.incoming_events.push_rejection(&server_addr);
                        //     self.disconnect_reset_connection();
                        //     return;
                        // }
                        None => {}
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(error) => {
                    self.incoming_world_events
                        .push_error(NaiaClientError::Wrapped(Box::new(error)));
                }
            }
        }

        return;
    }

    fn maintain_connection(&mut self) {
        // connection already established

        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Should have checked for this above");
        };

        Self::handle_heartbeats(connection, &mut self.io);
        Self::handle_pings(connection, &mut self.io);
        Self::handle_empty_acks(connection, &mut self.io);

        let mut received_any = false;
        let mut _packets_received = 0;

        // receive from socket
        loop {
            match self.io.recv_reader() {
                Ok(Some(mut reader)) => {
                    _packets_received += 1;
                    connection.mark_heard();

                    let header = match StandardHeader::de(&mut reader) {
                        Ok(h) => h,
                        Err(_e) => {
                            continue;
                        }
                    };
                    match header.packet_type {
                        PacketType::Data => {
                            // Count world packets received from transport
                            #[cfg(feature = "e2e_debug")]
                            {
                                use crate::counters::CLIENT_WORLD_PKTS_RECV;
                                use std::sync::atomic::Ordering;
                                CLIENT_WORLD_PKTS_RECV.fetch_add(1, Ordering::Relaxed);
                            }
                            // continue
                        }
                        PacketType::Heartbeat
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
                    received_any = true;
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
                            connection.base.mark_should_send_empty_ack();

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
                    self.incoming_world_events
                        .push_error(NaiaClientError::Wrapped(Box::new(error)));
                }
            }
        }

        if received_any {
            connection.process_received_commands();
        }
    }

    fn handle_heartbeats(connection: &mut Connection, io: &mut Io) {
        // send heartbeats
        if connection.base.should_send_heartbeat() {
            Self::send_heartbeat_packet(connection, io);
        }
    }

    fn handle_empty_acks(connection: &mut Connection, io: &mut Io) {
        // send empty acks
        if connection.base.should_send_empty_ack() {
            Self::send_heartbeat_packet(connection, io);
        }
    }

    fn send_heartbeat_packet(connection: &mut Connection, io: &mut Io) {
        let mut writer = BitWriter::new();

        // write header
        let _header = connection
            .base
            .write_header(PacketType::Heartbeat, &mut writer);

        // send packet
        if io.send_packet(writer.to_packet()).is_err() {
            // TODO: pass this on and handle above
            warn!("Client Error: Cannot send heartbeat packet to Server");
        }
        connection.mark_sent();
    }

    fn handle_pings(connection: &mut Connection, io: &mut Io) {
        // send pings
        if connection.time_manager.send_ping(io) {
            connection.mark_sent();
        }
    }

    fn disconnect_with_events<W: WorldMutType<E>>(&mut self, world: &mut W) {
        let server_addr = self.server_address_unwrapped();

        self.incoming_world_events.clear();
        self.incoming_tick_events.clear();

        self.despawn_all_remote_entities(world);
        self.disconnect_reset_connection();

        self.incoming_world_events.push_disconnection(&server_addr);
    }

    fn despawn_all_remote_entities<W: WorldMutType<E>>(&mut self, world: &mut W) {
        // this is very similar to the newtype method .. can we coalesce and reduce
        // duplication?

        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Client is already disconnected!");
        };

        let remote_entities = connection.base.world_manager.remote_entities();
        let entity_events = SharedGlobalWorldManager::despawn_all_entities(
            world,
            &self.global_entity_map,
            &self.global_world_manager,
            remote_entities,
        );
        self.process_entity_events(world, entity_events);
    }

    fn disconnect_reset_connection(&mut self) {
        self.server_connection = None;

        self.io = Io::new(
            &self.client_config.connection.bandwidth_measure_duration,
            &self.protocol.compression,
        );

        self.handshake_manager = Box::new(HandshakeManager::new(
            self.client_config.send_handshake_interval,
            self.client_config.ping_interval,
            self.client_config.handshake_pings,
        ));

        self.manual_disconnect = false;
        self.global_world_manager = GlobalWorldManager::new();
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        self.io.server_addr().expect("connection not established!")
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_diagnostic(&self, remote_entity: &naia_shared::RemoteEntity) -> Option<(naia_shared::EntityChannelState, (naia_shared::SubCommandId, usize, Option<naia_shared::SubCommandId>, usize))> {
        let Some(connection) = self.server_connection.as_ref() else {
            return None;
        };
        connection.base.world_manager.debug_remote_channel_diagnostic(remote_entity)
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_snapshot(&self, remote_entity: &naia_shared::RemoteEntity) -> Option<(naia_shared::EntityChannelState, Option<naia_shared::MessageIndex>, usize, Option<(naia_shared::MessageIndex, naia_shared::EntityMessageType)>, Option<naia_shared::MessageIndex>)> {
        let Some(connection) = self.server_connection.as_ref() else {
            return None;
        };
        connection.base.world_manager.debug_remote_channel_snapshot(remote_entity)
    }


    fn process_entity_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity_events: Vec<EntityEvent>,
    ) {
        for response_event in entity_events {
            // info!(
            //     "Client.process_entity_events(), handling response_event: {:?}",
            //     response_event.log()
            // );
            match response_event {
                EntityEvent::Spawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_spawn(world_entity);
                    self.global_world_manager
                        .remote_spawn_entity(&global_entity);
                    let Some(connection) = self.server_connection.as_mut() else {
                        panic!("Client is disconnected!");
                    };
                    connection
                        .base
                        .world_manager
                        .remote_spawn_entity(&global_entity); // TODO: move to localworld?
                    #[cfg(feature = "e2e_debug")]
                    {
                        use crate::counters::CLIENT_SCOPE_APPLIED_ADD_E2;
                        use std::sync::atomic::Ordering;
                        CLIENT_SCOPE_APPLIED_ADD_E2.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_despawn(world_entity);
                    if self
                        .global_world_manager
                        .entity_is_delegated(&global_entity)
                    {
                        if let Some(status) = self
                            .global_world_manager
                            .entity_authority_status(&global_entity)
                        {
                            if status != EntityAuthStatus::Available {
                                self.entity_update_authority(
                                    &global_entity,
                                    &world_entity,
                                    EntityAuthStatus::Available,
                                );
                            }
                        }
                    }
                    self.global_world_manager
                        .remove_entity_record(&global_entity);
                    self.global_entity_map.despawn_by_global(&global_entity);
                    #[cfg(feature = "e2e_debug")]
                    {
                        use crate::counters::CLIENT_SCOPE_APPLIED_REMOVE_E1;
                        use std::sync::atomic::Ordering;
                        CLIENT_SCOPE_APPLIED_REMOVE_E1.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::InsertComponent(global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_insert(world_entity, component_kind);

                    if !self
                        .global_world_manager
                        .entity_has_component(&global_entity, &component_kind)
                    {
                        if self
                            .global_world_manager
                            .entity_is_delegated(&global_entity)
                        {
                            // let component_name = self
                            //     .protocol
                            //     .component_kinds
                            //     .kind_to_name(&component_kind);
                            // info!(
                            //     "Client.process_response_events(), handling InsertComponent for Component: {:?} into delegated Entity: {:?}",
                            //     component_name, global_entity
                            // );
                            world.component_publish(
                                &self.protocol.component_kinds,
                                &self.global_entity_map,
                                &self.global_world_manager,
                                &world_entity,
                                &component_kind,
                            );
                            world.component_enable_delegation(
                                &self.protocol.component_kinds,
                                &self.global_entity_map,
                                &self.global_world_manager,
                                &world_entity,
                                &component_kind,
                            );
                        }

                        self.global_world_manager
                            .remote_insert_component(&global_entity, &component_kind);
                    }
                }
                EntityEvent::RemoveComponent(global_entity, component_box) => {
                    let component_kind = component_box.kind();
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_remove(world_entity, component_box);
                    if self
                        .global_world_manager
                        .entity_is_delegated(&global_entity)
                    {
                        self.remove_component_worldless(&world_entity, &component_kind);
                    } else {
                        self.global_world_manager
                            .remove_component_record(&global_entity, &component_kind);
                    }
                }
                EntityEvent::Publish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.publish_entity(&global_entity, false);
                    self.incoming_world_events.push_publish(world_entity);
                }
                EntityEvent::Unpublish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.unpublish_entity(&global_entity, false);
                    self.incoming_world_events.push_unpublish(world_entity);
                }
                EntityEvent::EnableDelegation(global_entity) => {
                    #[cfg(feature = "e2e_debug")]
                    naia_shared::e2e_trace!(
                        "[CLIENT_RECV] EnableDelegation entity={:?}",
                        global_entity
                    );
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();

                    self.entity_enable_delegation(world, &global_entity, &world_entity, false);

                    // Send EnableDelegationEntityResponse action via EntityActionEvent system
                    let Some(connection) = &mut self.server_connection else {
                        return;
                    };
                    connection
                        .base
                        .world_manager
                        .send_enable_delegation_response(&global_entity); // TODO: move to localworld?
                }
                EntityEvent::EnableDelegationResponse(_) => {
                    panic!("Client should never receive an EnableDelegationEntityResponse event");
                }
                EntityEvent::DisableDelegation(global_entity) => {
                    #[cfg(feature = "e2e_debug")]
                    {
                        let delegated_at_entry = self
                            .global_world_manager
                            .entity_is_delegated(&global_entity);
                        naia_shared::e2e_trace!(
                            "[CLIENT_RECV] DisableDelegation entity={:?} delegated_at_entry={}",
                            global_entity, delegated_at_entry
                        );
                    }
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.entity_disable_delegation(world, &global_entity, &world_entity, false);
                }
                EntityEvent::RequestAuthority(_global_entity) => {
                    panic!("Client should never receive an EntityRequestAuthority event");
                }
                EntityEvent::ReleaseAuthority(_global_entity) => {
                    panic!("Client should never receive an EntityReleaseAuthority event");
                }
                EntityEvent::SetAuthority(global_entity, new_auth_status) => {
                    // Count when SetAuthority successfully converts to EntityEvent (after mapping)
                    #[cfg(feature = "e2e_debug")]
                    if new_auth_status == EntityAuthStatus::Granted {
                        use crate::counters::{CLIENT_RX_SET_AUTH, CLIENT_TO_EVENT_SET_AUTH_OK};
                        use std::sync::atomic::Ordering;
                        CLIENT_RX_SET_AUTH.fetch_add(1, Ordering::Relaxed);
                        CLIENT_TO_EVENT_SET_AUTH_OK.fetch_add(1, Ordering::Relaxed);
                    }
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.entity_update_authority(&global_entity, &world_entity, new_auth_status);
                }
                EntityEvent::MigrateResponse(global_entity, new_remote_entity) => {
                    // Validate we have a valid world entity
                    let world_entity = match self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                    {
                        Ok(entity) => entity,
                        Err(_) => {
                            warn!(
                                "Received MigrateResponse for unknown global entity: {:?}",
                                global_entity
                            );
                            return;
                        }
                    };

                    // Scope the connection borrow to complete migration steps
                    {
                        let Some(connection) = &mut self.server_connection else {
                            warn!("Received MigrateResponse without active server connection");
                            return;
                        };

                        let old_host_entity = match connection
                            .base
                            .world_manager
                            .entity_converter()
                            .global_entity_to_host_entity(&global_entity)
                        {
                            Ok(entity) => entity,
                            Err(_) => {
                                warn!(
                                    "Entity {:?} does not exist as HostEntity before migration",
                                    global_entity
                                );
                                return;
                            }
                        };

                        // Extract and buffer outgoing commands to preserve pending operations
                        let buffered_commands = connection
                            .base
                            .world_manager
                            .extract_host_entity_commands(&global_entity);

                        // Extract component state to preserve during migration
                        let component_kinds = connection
                            .base
                            .world_manager
                            .extract_host_component_kinds(&global_entity);

                        // Remove old HostEntityChannel
                        connection
                            .base
                            .world_manager
                            .remove_host_entity(&global_entity);

                        // Create new RemoteEntityChannel with preserved component state
                        connection.base.world_manager.insert_remote_entity(
                            &global_entity,
                            new_remote_entity,
                            component_kinds,
                        );

                        // Install entity redirect for old references
                        let old_entity = OwnedLocalEntity::Host(old_host_entity.value());
                        let new_entity = OwnedLocalEntity::Remote(new_remote_entity.value());
                        connection
                            .base
                            .world_manager
                            .install_entity_redirect(old_entity, new_entity);

                        // Update pending command packet references
                        connection
                            .base
                            .world_manager
                            .update_sent_command_entity_refs(
                                &global_entity,
                                old_entity,
                                new_entity,
                            );

                        // Replay buffered commands
                        for command in buffered_commands {
                            if command.is_valid_for_remote_entity() {
                                connection
                                    .base
                                    .world_manager
                                    .replay_entity_command(&global_entity, command);
                            }
                        }

                        // Update RemoteEntityChannel's internal AuthChannel status
                        // After migration, grant authority back to the creating client
                        connection
                            .base
                            .world_manager
                            .remote_receive_set_auth(&global_entity, EntityAuthStatus::Granted);
                    }

                    // Complete delegation in global world manager
                    self.entity_complete_delegation(world, &global_entity, &world_entity);

                    // Update global authority status
                    self.global_world_manager
                        .entity_update_authority(&global_entity, EntityAuthStatus::Granted);

                    // Emit AuthGrant event
                    self.incoming_world_events.push_auth_grant(world_entity);
                    #[cfg(feature = "e2e_debug")]
                    {
                        use crate::counters::CLIENT_EMIT_AUTH_GRANTED_EVENT;
                        use std::sync::atomic::Ordering;
                        CLIENT_EMIT_AUTH_GRANTED_EVENT.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::UpdateComponent(tick, global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_update(tick, world_entity, component_kind);
                }
            }
        }
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> EntityAndGlobalEntityConverter<E> for Client<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.global_entity_map
            .global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(
        &self,
        world_entity: &E,
    ) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.global_entity_map.entity_to_global_entity(world_entity)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

impl ConnectionStatus {
    pub fn is_disconnected(&self) -> bool {
        self == &ConnectionStatus::Disconnected
    }

    pub fn is_connecting(&self) -> bool {
        self == &ConnectionStatus::Connecting
    }

    pub fn is_connected(&self) -> bool {
        self == &ConnectionStatus::Connected
    }

    pub fn is_disconnecting(&self) -> bool {
        self == &ConnectionStatus::Disconnecting
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::LocalEntity;

        impl<E: Copy + Eq + Hash + Send + Sync> Client<E> {
            /// Returns all LocalEntity IDs for entities replicated to the server.
            ///
            /// Returns the set of LocalEntity IDs that currently exist for the server
            /// (i.e., all entities replicated to the server).
            /// The ordering doesn't matter.
            ///
            /// # Panics
            /// Panics if not connected to server
            pub fn local_entities(&self) -> Vec<LocalEntity> {
                let connection = self
                    .server_connection
                    .as_ref()
                    .expect("Server connection does not exist");

                connection.base.world_manager.local_entities()
            }

            /// Retrieves an EntityRef that exposes read-only operations for the Entity
            /// identified by the given LocalEntity for the server.
            ///
            /// Returns `None` if:
            /// - The server is not connected
            /// - The LocalEntity doesn't exist for the server
            /// - The entity does not exist in the world
            pub fn local_entity<W: WorldRefType<E>>(
                &self,
                world: W,
                local_entity: &LocalEntity,
            ) -> Option<EntityRef<'_, E, W>> {
                let world_entity = self.local_to_world_entity(local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity(world, &world_entity))
            }

            /// Retrieves an EntityMut that exposes read and write operations for the Entity
            /// identified by the given LocalEntity for the server.
            ///
            /// Returns `None` if:
            /// - The server is not connected
            /// - The LocalEntity doesn't exist for the server
            /// - The entity does not exist in the world
            pub fn local_entity_mut<W: WorldMutType<E>>(
                &mut self,
                world: W,
                local_entity: &LocalEntity,
            ) -> Option<EntityMut<'_, E, W>> {
                let world_entity = self.local_to_world_entity(local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity_mut(world, &world_entity))
            }

            fn local_to_world_entity(
                &self,
                local_entity: &LocalEntity
            ) -> Option<E> {
                let connection = self.server_connection.as_ref()?;
                let converter = connection.base.world_manager.entity_converter();

                let owned_local_entity: OwnedLocalEntity = (*local_entity).into();
                let global_entity = converter.owned_entity_to_global_entity(&owned_local_entity).ok()?;
                let world_entity = self
                    .global_entity_map
                    .global_entity_to_entity(&global_entity)
                    .ok()?;

                Some(world_entity)
            }

            pub(crate) fn world_to_local_entity(
                &self,
                world_entity: &E,
            ) -> Option<LocalEntity> {
                let global_entity = self.global_entity_map.entity_to_global_entity(world_entity).ok()?;

                let connection = self.server_connection.as_ref()?;
                let converter = connection.base.world_manager.entity_converter();
                let owned_entity = converter.global_entity_to_owned_entity(&global_entity).ok()?;

                Some(LocalEntity::from(owned_entity))
            }
        }
    }
}
