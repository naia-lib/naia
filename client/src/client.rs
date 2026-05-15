use std::{any::Any, collections::VecDeque, hash::Hash, net::SocketAddr, time::Duration};

use log::{debug, info, warn};

use naia_shared::{
    handshake::{HandshakeHeader, RejectReason},
    AuthorityError, BitWriter, Channel, ChannelKind, ComponentKind, ConnectionStats,
    EntityAndGlobalEntityConverter,
    EntityAuthStatus, EntityDoesNotExistError, EntityEvent, EntityPriorityMut, EntityPriorityRef,
    FakeEntityConverter, GameInstant, GlobalEntity, GlobalEntityMap, GlobalEntitySpawner,
    GlobalRequestId, GlobalResponseId, GlobalWorldManagerType, HostType, Instant, Message,
    MessageContainer, OwnedLocalEntity, PacketType, Protocol, ProtocolId, Replicate,
    ReplicatedComponent, Request, Response, ResponseReceiveKey, ResponseSendKey, Serde,
    SharedGlobalWorldManager, SocketConfig, StandardHeader, Tick, UserPriorityState, WorldMutType,
    WorldRefType,
};

use super::{
    client_config::ClientConfig, error::NaiaClientError, world_events::Events,
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
    Publicity,
};

/// The naia client — connects to a server, receives replicated entities, and
/// sends client-authoritative mutations and messages.
///
/// `E` is your world's entity key type (e.g. a `u32` or ECS `Entity`). It must
/// be `Copy + Eq + Hash + Send + Sync`.
///
/// # Minimal client loop
///
/// ```text
/// loop {
///     client.receive_all_packets();                      // 1. read UDP/WebRTC
///     client.process_all_packets(&mut world, &now);      // 2. decode + dispatch
///     for event in client.take_world_events() { ... }   // 3. handle events
///     for event in client.take_tick_events(&now) { ... } // 4. advance ticks
///     // apply predicted state here
///     client.send_all_packets(&world);                   // 5. flush outbound
/// }
/// ```
///
/// Steps 1–5 must run in this order every frame. Call [`auth`](Client::auth)
/// and then [`connect`](Client::connect) once before entering the loop.
pub struct Client<E: Copy + Eq + Hash + Send + Sync> {
    // Config
    client_config: ClientConfig,
    protocol: Protocol,
    protocol_id: ProtocolId,
    // Connection
    auth_message: Option<Vec<u8>>,
    auth_headers: Option<Vec<(String, String)>>,
    io: Io,
    server_connection: Option<Connection>,
    handshake_manager: Box<dyn Handshaker>,
    manual_disconnect: bool,
    server_disconnect: bool,
    waitlist_messages: VecDeque<(ChannelKind, Box<dyn Message>)>,
    // World
    global_world_manager: GlobalWorldManager,
    global_entity_map: GlobalEntityMap<E>,
    // Events
    incoming_world_events: Events<E>,
    incoming_tick_events: TickEvents,
    // Per-connection priority layer (single connection; no global/per-user split).
    priority: UserPriorityState<E>,
    // Replicated Resources — client-side mirror of the server's
    // ResourceRegistry. Populated when an InsertComponent for a
    // resource-marked component kind arrives; consulted by the bevy
    // adapter's mirror system to translate component events into
    // resource events and maintain the bevy-Resource side. See
    // `_AGENTS/RESOURCES_PLAN.md` §A1 + `RESOURCES_AUDIT.md`.
    resource_registry: naia_shared::ResourceRegistry,
}

impl<E: Copy + Eq + Hash + Send + Sync> Client<E> {
    /// Creates a new client with the given config and protocol.
    ///
    /// Call [`auth`](Client::auth) (optional) and then
    /// [`connect`](Client::connect) before entering the main loop.
    pub fn new<P: Into<Protocol>>(client_config: ClientConfig, protocol: P) -> Self {
        let mut protocol: Protocol = protocol.into();
        protocol.lock();
        let protocol_id = protocol.protocol_id();
        Self::new_with_protocol_id(client_config, protocol, protocol_id)
    }

    /// Creates a new client with an explicit protocol ID.
    ///
    /// # Adapter use only
    ///
    /// Bevy and macroquad adapters use this to inject a pre-computed ID.
    /// Prefer [`new`](Client::new) in application code.
    pub fn new_with_protocol_id(
        client_config: ClientConfig,
        protocol: Protocol,
        protocol_id: ProtocolId,
    ) -> Self {
        let handshake_manager = HandshakeManager::new(
            protocol_id,
            client_config.send_handshake_interval,
            client_config.ping_interval,
            client_config.handshake_pings,
        );

        let compression_config = protocol.compression.clone();

        let mut global_world_manager = GlobalWorldManager::new();
        global_world_manager.init_protocol_kind_count(protocol.component_kinds.kind_count());

        Self {
            // Config
            client_config: client_config.clone(),
            protocol,
            protocol_id,
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
            server_disconnect: false,
            waitlist_messages: VecDeque::new(),
            // World
            global_world_manager,
            global_entity_map: GlobalEntityMap::new(),
            // Events
            incoming_world_events: Events::new(),
            incoming_tick_events: TickEvents::new(),
            priority: UserPriorityState::new(),
            resource_registry: naia_shared::ResourceRegistry::new(),
        }
    }

    // Priority

    /// Read-only handle to the priority state for `entity` on this client's
    /// outbound connection.
    pub fn entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        self.priority.get_ref(entity)
    }

    /// Mutable handle to the priority state for `entity` on this client's
    /// outbound connection. Lazy-creates an entry on first write.
    pub fn entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        self.priority.get_mut(entity)
    }

    /// Stores the authentication message to send during the handshake.
    ///
    /// Must be called before [`connect`](Client::connect) if the server
    /// requires authentication. The server receives this as an
    /// [`AuthEvent`] in its connection handler.
    ///
    /// [`AuthEvent`]: naia_server::events::AuthEvent
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

    /// Stores HTTP-style key-value headers to include in the WebRTC upgrade
    /// request.
    ///
    /// Used by WebRTC transports that support header-based authentication or
    /// routing. Ignored by native UDP sockets.
    pub fn auth_headers(&mut self, headers: Vec<(String, String)>) {
        self.auth_headers = Some(headers);
    }

    /// Opens the socket and begins the handshake with the server.
    ///
    /// If [`auth`](Client::auth) was called, the auth payload is included in
    /// the handshake. After connecting, process events via the main loop
    /// until a [`ConnectionEvent`] arrives.
    ///
    /// # Panics
    ///
    /// Panics if the client has already initiated a connection. Check
    /// [`connection_status`](Client::connection_status) before calling.
    ///
    /// [`ConnectionEvent`]: crate::events::ConnectionEvent
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
        } else if let Some(auth_headers) = &self.auth_headers {
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

    /// Returns the client's current connection lifecycle state.
    ///
    /// Transitions: `Disconnected` → `Connecting` (after
    /// [`connect`](Client::connect)) → `Connected` (after handshake) →
    /// `Disconnecting` (after [`disconnect`](Client::disconnect)) →
    /// `Disconnected`.
    pub fn connection_status(&self) -> ConnectionStatus {
        if self.is_connected() {
            if self.is_disconnecting() {
                ConnectionStatus::Disconnecting
            } else {
                ConnectionStatus::Connected
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
            connection.should_drop() || self.manual_disconnect || self.server_disconnect
        } else {
            false
        }
    }

    /// Returns whether or not the client is disconnected
    fn is_disconnected(&self) -> bool {
        !self.io.is_loaded()
    }

    /// Initiates a clean disconnect from the server.
    ///
    /// Sends several disconnect packets to increase delivery probability,
    /// then begins the disconnection process. A [`DisconnectionEvent`] is
    /// emitted on the next [`take_world_events`](Client::take_world_events)
    /// call.
    ///
    /// # Panics
    ///
    /// Panics if the client is not currently connected.
    ///
    /// [`DisconnectionEvent`]: crate::events::DisconnectionEvent
    pub fn disconnect(&mut self) {
        if !self.is_connected() {
            panic!("Trying to disconnect Client which is not connected yet!")
        }

        for _ in 0..10 {
            let writer = self.handshake_manager.write_disconnect();
            if self.io.send_packet(writer.to_packet()).is_err() {
                // Best-effort: we send 10 disconnect packets and move on.
                // If none reach the server it will time out the connection anyway.
                warn!("Client Error: Cannot send disconnect packet to Server");
            }
        }

        self.manual_disconnect = true;
    }

    /// Returns the socket configuration from the protocol.
    pub fn socket_config(&self) -> &SocketConfig {
        &self.protocol.socket
    }

    // Event loop ────────────────────────────────────────────────────────────

    /// Reads all pending packets from the socket.
    ///
    /// Must be called **first** in the client loop, before
    /// [`process_all_packets`](Client::process_all_packets). Handles
    /// handshake progress, heartbeats, and buffers incoming data packets.
    pub fn receive_all_packets(&mut self) {
        // Need to run this to maintain connection with server, and receive packets
        // until none left
        self.maintain_socket();
    }

    /// Decodes all buffered packets and applies changes to the world.
    ///
    /// Must be called after [`receive_all_packets`](Client::receive_all_packets)
    /// and before [`take_world_events`](Client::take_world_events). Applies
    /// server-replicated entity spawn/update/despawn events and queues them
    /// for the next [`take_world_events`] call.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        // all other operations
        if self.is_disconnecting() {
            let reason = if self.manual_disconnect || self.server_disconnect {
                naia_shared::DisconnectReason::ClientDisconnected
            } else {
                naia_shared::DisconnectReason::TimedOut
            };
            self.disconnect_with_events(&mut world, reason);
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
            now,
            &mut self.incoming_world_events,
        );

        self.process_entity_events(&mut world, entity_events);
    }

    /// Drains and returns all accumulated world events since the last call.
    ///
    /// Must be called after [`process_all_packets`](Client::process_all_packets).
    /// The returned [`Events`] contains entity spawn/despawn/update notifications,
    /// message arrivals, connection/disconnection signals, and authority events.
    /// Not calling this causes the buffer to grow without bound.
    ///
    /// [`Events`]: crate::Events
    pub fn take_world_events(&mut self) -> Events<E> {
        std::mem::take(&mut self.incoming_world_events)
    }

    /// Advances the tick clocks and returns any tick-boundary events.
    ///
    /// Must be called after [`take_world_events`](Client::take_world_events).
    /// Returns a [`TickEvents`] containing client and server tick advances
    /// since the last call. Also de-jitters buffered packets on tick
    /// boundaries (unless the jitter buffer is in bypass mode).
    ///
    /// [`TickEvents`]: crate::TickEvents
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        let Some(connection) = &mut self.server_connection else {
            return TickEvents::default();
        };

        let (receiving_tick_happened, sending_tick_happened) =
            connection.time_manager.collect_ticks(now);

        // If jitter buffer is in bypass mode, process packets immediately regardless of tick
        // Otherwise, only process on tick boundaries
        let should_read_packets = match self.client_config.jitter_buffer {
            JitterBufferType::Bypass => true,
            JitterBufferType::Real => receiving_tick_happened.is_some(),
        };

        if should_read_packets {
            // read packets on tick boundary, de-jittering
            if let Err(_err) = connection.read_buffered_packets(
                &self.protocol.channel_kinds,
                &self.protocol.message_kinds,
                &self.protocol.component_kinds,
            ) {
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

    /// Flushes all queued messages and entity mutations to the server.
    ///
    /// Must be called **last** in the client loop. Serialises outbound
    /// packets and hands them to the transport. Also handles handshake
    /// packet retransmission when not yet connected. If this is not called,
    /// the server never receives any updates.
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
        } else if self.io.is_loaded() {
            if let Some(outgoing_packet) = self.handshake_manager.send() {
                if self.io.send_packet(outgoing_packet).is_err() {
                    // Single handshake send failure is not fatal: the handshake
                    // manager retries on the next tick until the server responds.
                    warn!("Client Error: Cannot send handshake packet to Server");
                }
            }
        }
    }

    // Messaging ─────────────────────────────────────────────────────────────

    /// Queues a message to be sent to the server on the next
    /// [`send_all_packets`](Client::send_all_packets) call.
    ///
    /// `C` is the channel type (ordering and reliability). `M` is the message
    /// type (must be registered in the [`Protocol`]). Messages sent before
    /// the connection is established are queued and delivered on connect.
    ///
    /// # Errors
    ///
    /// Returns an error if the channel does not allow client-to-server
    /// messages, or if the channel is `TickBuffered` (use
    /// [`send_tick_buffer_message`](Client::send_tick_buffer_message) instead).
    ///
    /// [`Protocol`]: naia_shared::Protocol
    pub fn send_message<C: Channel, M: Message>(
        &mut self,
        message: &M,
    ) -> Result<(), NaiaClientError> {
        let cloned_message = M::clone_box(message);
        self.send_message_inner(&ChannelKind::of::<C>(), cloned_message)
    }

    fn send_message_inner(
        &mut self,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) -> Result<(), NaiaClientError> {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);
        if !channel_settings.can_send_to_server() {
            return Err(NaiaClientError::Message(
                "Cannot send message to Server on this Channel".to_string(),
            ));
        }

        if channel_settings.tick_buffered() {
            return Err(NaiaClientError::Message("Cannot call `Client.send_message()` on a Tick Buffered Channel, use `Client.send_tick_buffered_message()` instead".to_string()));
        }

        if let Some(connection) = &mut self.server_connection {
            let mut converter = connection
                .base
                .world_manager
                .entity_converter_mut(&self.global_world_manager);
            let message = MessageContainer::new(message_box);
            let accepted = connection.base.message_manager.send_message(
                &self.protocol.message_kinds,
                &mut converter,
                channel_kind,
                message,
            );
            if !accepted {
                return Err(NaiaClientError::MessageQueueFull);
            }
        } else {
            self.waitlist_messages
                .push_back((*channel_kind, message_box));
        }
        Ok(())
    }

    /// Sends a request to the server and returns a key for polling the
    /// response.
    ///
    /// Use [`receive_response`](Client::receive_response) with the returned
    /// key to collect the server's reply.
    ///
    /// # Errors
    ///
    /// Returns an error if the client is not currently connected.
    ///
    /// # Panics
    ///
    /// Panics if the channel is not bidirectional and reliable.
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
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);

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
        let message = MessageContainer::new(request_box);
        connection.base.message_manager.send_request(
            &self.protocol.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            message,
        );

        Ok(request_id)
    }

    /// Sends a response to the server's request.
    ///
    /// `response_key` is obtained from the [`RequestEvent`] that delivered
    /// the server's original request. Returns `true` on success; `false` if
    /// the key is no longer valid (e.g. the connection was dropped).
    ///
    /// [`RequestEvent`]: crate::events::RequestEvent
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

        let response = MessageContainer::new(response_box);
        connection.base.message_manager.send_response(
            &self.protocol.message_kinds,
            &mut converter,
            &channel_kind,
            local_response_id,
            response,
        );
        true
    }

    /// Returns `true` if a response to the given request has arrived.
    ///
    /// Non-destructive — does not consume the response. Call
    /// [`receive_response`](Client::receive_response) to retrieve and consume
    /// it.
    pub fn has_response<S: Response>(&self, response_key: &ResponseReceiveKey<S>) -> bool {
        let Some(connection) = &self.server_connection else {
            return false;
        };
        let request_id = response_key.request_id();
        connection.global_request_manager.has_response(&request_id)
    }

    /// Polls for and consumes a response to a previously sent client request.
    ///
    /// Returns `Some(response)` once the server replies, or `None` if the
    /// response has not yet arrived or the key is invalid. The key is
    /// invalidated after a successful receive.
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<S> {
        let Some(connection) = &mut self.server_connection else {
            return None;
        };
        let request_id = response_key.request_id();
        let container = connection
            .global_request_manager
            .destroy_request_id(&request_id)?;
        let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
            .ok()
            .map(|boxed_s| *boxed_s)
            .unwrap();
        Some(response)
    }
    //

    fn on_connect(&mut self) {
        // send queued messages
        let messages = std::mem::take(&mut self.waitlist_messages);
        for (channel_kind, message_box) in messages {
            let _ = self.send_message_inner(&channel_kind, message_box);
        }
    }

    /// Queues a tick-buffered message stamped with the given client tick.
    ///
    /// Use this for client input on a [`TickBuffered`] channel. The server
    /// receives the message when its tick counter reaches the stamped tick,
    /// enabling tick-accurate input replay.
    ///
    /// # Panics
    ///
    /// Panics if the channel does not have `TickBuffered` mode enabled.
    ///
    /// [`TickBuffered`]: naia_shared::ChannelMode::TickBuffered
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
            let message = MessageContainer::new(message_box);
            connection
                .tick_buffer
                .send_message(tick, channel_kind, message);
        }
    }

    // Entities ──────────────────────────────────────────────────────────────

    /// Spawns a client-owned entity and returns a builder for configuring it.
    ///
    /// The spawned entity starts as [`Private`](naia_shared::Publicity::Private);
    /// call [`configure_replication`](crate::EntityMut::configure_replication)
    /// on the returned [`EntityMut`] to publish it.
    ///
    /// Requires that the protocol was built with
    /// `enable_client_authoritative_entities()`.
    ///
    /// # Panics
    ///
    /// Panics if client-authoritative entities are not enabled in the protocol.
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, mut world: W) -> EntityMut<'_, E, W> {
        self.check_client_authoritative_allowed();

        let world_entity = world.spawn_entity();

        self.spawn_entity_inner(&world_entity);

        EntityMut::new(self, world, &world_entity)
    }

    /// Creates a new static entity.
    ///
    /// A full component snapshot is sent once when the entity enters the server's scope;
    /// no diff-tracking occurs thereafter. Use for client-owned entities that are
    /// write-once after spawn (e.g. tiles, level geometry sent to the server).
    ///
    /// Equivalent to `spawn_entity(world).as_static()`, but avoids registering
    /// in the dynamic pool first.
    pub fn spawn_static_entity<W: WorldMutType<E>>(
        &'_ mut self,
        mut world: W,
    ) -> EntityMut<'_, E, W> {
        self.check_client_authoritative_allowed();

        let world_entity = world.spawn_entity();

        self.spawn_static_entity_inner(&world_entity);

        let mut entity_mut = EntityMut::new(self, world, &world_entity);
        entity_mut.allow_static_insert = true;
        entity_mut
    }

    /// Creates a new Entity with a specific id
    fn spawn_entity_inner(&mut self, world_entity: &E) {
        self.spawn_entity_inner_with_static(world_entity, false);
    }

    fn spawn_static_entity_inner(&mut self, world_entity: &E) {
        self.spawn_entity_inner_with_static(world_entity, true);
    }

    fn spawn_entity_inner_with_static(&mut self, world_entity: &E, is_static: bool) {
        let global_entity = self.global_entity_map.spawn(*world_entity, None);

        if is_static {
            self.global_world_manager.host_spawn_static_entity(&global_entity);
        } else {
            self.global_world_manager.host_spawn_entity(&global_entity);
        }

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
            .host_init_entity(&global_entity, component_kinds, &self.protocol.component_kinds, is_static);
    }

    // Replicated Resources (client-side mirror) ─────────────────────────────
    // Populated when the remote-apply path delivers an InsertComponent for a
    // resource kind. Clears on Despawn. The Bevy adapter consumes this to
    // drive the Bevy-Resource mirror (see adapters/bevy/client/src/resource_sync).

    /// Returns `true` if the client has a server-replicated resource of type
    /// `R` currently in scope.
    pub fn has_resource<R: 'static>(&self) -> bool {
        self.resource_registry.entity_for::<R>().is_some()
    }

    /// O(1): the world-entity carrying resource `R` on this client,
    /// or `None` if not currently in scope.
    pub fn resource_entity<R: 'static>(&self) -> Option<E> {
        let global_entity = self.resource_registry.entity_for::<R>()?;
        self.global_entity_map
            .global_entity_to_entity(&global_entity)
            .ok()
    }

    /// True iff `world_entity` is the entity carrying any Replicated
    /// Resource currently in scope on this client.
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity)
        else {
            return false;
        };
        self.resource_registry.is_resource_entity(&global_entity)
    }

    /// Number of currently-mirrored Replicated Resources.
    pub fn resources_count(&self) -> usize {
        self.resource_registry.len()
    }

    /// Iterate over the world-entities of all currently-mirrored resources.
    pub fn resource_entities(&self) -> Vec<E> {
        let mut out = Vec::with_capacity(self.resource_registry.len());
        for global_entity in self.resource_registry.entities() {
            if let Ok(e) = self
                .global_entity_map
                .global_entity_to_entity(global_entity)
            {
                out.push(e);
            }
        }
        out
    }

    /// Returns a read-only handle to the entity.
    ///
    /// # Panics
    ///
    /// Panics if the entity does not exist in the world.
    pub fn entity<W: WorldRefType<E>>(&'_ self, world: W, entity: &E) -> EntityRef<'_, E, W> {
        if world.has_entity(entity) {
            return EntityRef::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Returns a mutable handle to the entity.
    ///
    /// # Panics
    ///
    /// Panics if the entity does not exist in the world, or if
    /// client-authoritative entities are not enabled in the protocol.
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

    /// Returns all entities currently present in the world.
    pub fn entities<W: WorldRefType<E>>(&self, world: &W) -> Vec<E> {
        world.entities()
    }

    pub(crate) fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        if let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) {
            if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
                return owner;
            }
        }
        EntityOwner::Local
    }

    // Authority and replication config ──────────────────────────────────────

    /// Registers the entity with the replication layer.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter when a [`Replicate`] component is inserted.
    /// Use [`spawn_entity`](Client::spawn_entity) in application code.
    ///
    /// [`Replicate`]: naia_shared::Replicate
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        self.spawn_entity_inner(entity);
    }

    /// Registers the entity as static with the replication layer.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter's `as_static()` command. Use
    /// [`spawn_static_entity`](Client::spawn_static_entity) in application code.
    pub fn enable_static_entity_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        self.spawn_static_entity_inner(entity);
    }

    /// Converts an already-registered dynamic entity to static.
    ///
    /// Only safe to call before the server connection is established; after that
    /// the entity has already been initialized in the dynamic ID pool.
    ///
    /// # Adapter use only
    ///
    /// Use [`spawn_static_entity`](Client::spawn_static_entity) in application code.
    pub fn mark_entity_as_static(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(entity) else {
            panic!("entity not found in global map");
        };
        self.global_world_manager.mark_entity_as_static(&global_entity);
    }

    /// Unregisters the entity from the replication layer.
    ///
    /// # Adapter use only
    ///
    /// Called by the Bevy adapter when a [`Replicate`] component is removed.
    ///
    /// [`Replicate`]: naia_shared::Replicate
    pub fn disable_entity_replication(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(entity);
    }

    /// Returns the current [`Publicity`] for the entity, or `None` if the
    /// entity is not registered.
    ///
    /// # Adapter use only
    ///
    /// Use [`EntityRef::replication_config`](crate::EntityRef::replication_config)
    /// in application code.
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<Publicity> {
        self.check_client_authoritative_allowed();
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .entity_replication_config(&global_entity)
    }

    /// Returns `true` if the entity is registered as static.
    pub(crate) fn entity_is_static(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return false;
        };
        self.global_world_manager.entity_is_static(&global_entity)
    }

    /// Updates the replication config for a client-owned entity.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).configure_replication(config)`](crate::EntityMut::configure_replication)
    /// instead.
    ///
    /// # Panics
    ///
    /// Panics if the entity is server-owned, not yet replicating, or if the
    /// entity is already `Delegated`.
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: Publicity,
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
            // Already in the desired state, no-op
            return;
        }
        match prev_config {
            Publicity::Private => {
                match next_config {
                    Publicity::Private => {
                        panic!("This should not be possible.");
                    }
                    Publicity::Public => {
                        // private -> public
                        self.publish_entity(&global_entity, true);
                    }
                    Publicity::Delegated => {
                        // private -> delegated
                        self.publish_entity(&global_entity, true);
                        self.entity_enable_delegation(world, &global_entity, world_entity, true);
                    }
                }
            }
            Publicity::Public => {
                match next_config {
                    Publicity::Private => {
                        // public -> private
                        self.unpublish_entity(&global_entity, true);
                    }
                    Publicity::Public => {
                        panic!("This should not be possible.");
                    }
                    Publicity::Delegated => {
                        // public -> delegated
                        self.entity_enable_delegation(world, &global_entity, world_entity, true);
                    }
                }
            }
            Publicity::Delegated => {
                panic!(
                    "Delegated Entities are always ultimately Server-owned. Client cannot modify."
                )
            }
        }
    }

    /// Returns the current authority status for the entity from the client's
    /// perspective, or `None` if the entity is not delegable.
    ///
    /// # Adapter use only
    ///
    /// Application code should inspect authority via [`EntityRef::authority`](crate::EntityRef::authority).
    pub fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        self.check_client_authoritative_allowed();

        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return None;
        };

        self.global_world_manager.entity_authority_status(&global_entity)
    }

    /// Sends an authority request to the server for the given delegated entity.
    ///
    /// The server responds with either [`EntityAuthGrantedEvent`] or
    /// [`EntityAuthDeniedEvent`]. Only valid for entities with
    /// [`Delegated`](naia_shared::Publicity::Delegated) replication config.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).request_authority()`](crate::EntityMut::request_authority)
    /// instead.
    ///
    /// [`EntityAuthGrantedEvent`]: crate::events::EntityAuthGrantedEvent
    /// [`EntityAuthDeniedEvent`]: crate::events::EntityAuthDeniedEvent
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

    /// Releases the client's authority over the given entity back to the
    /// server.
    ///
    /// Only valid when this client holds `Granted` authority. The server
    /// resumes ownership after confirming the release.
    ///
    /// # Adapter use only
    ///
    /// Application code should call
    /// [`entity_mut(...).release_authority()`](crate::EntityMut::release_authority)
    /// instead.
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

    // Connection ────────────────────────────────────────────────────────────

    /// Returns the server's socket address.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection has not been established yet.
    pub fn server_address(&self) -> Result<SocketAddr, NaiaClientError> {
        self.io.server_addr()
    }

    /// Returns the rolling-average round-trip time (seconds) to the server.
    ///
    /// Returns `0.0` if the connection has not been established yet.
    pub fn rtt(&self) -> f32 {
        self.server_connection
            .as_ref()
            .map(|conn| conn.time_manager.rtt() / 1000.0)
            .unwrap_or(0.0)
    }

    /// Returns the rolling-average jitter (seconds) measured for the server
    /// connection.
    ///
    /// Returns `0.0` if the connection has not been established yet.
    pub fn jitter(&self) -> f32 {
        self.server_connection
            .as_ref()
            .map(|conn| conn.time_manager.jitter() / 1000.0)
            .unwrap_or(0.0)
    }

    // Ticks ─────────────────────────────────────────────────────────────────

    /// Returns the client's current sending tick, or `None` if not connected.
    ///
    /// This is the tick at which the client is currently sending — use it to
    /// stamp [`TickBuffered`] messages for prediction.
    ///
    /// [`TickBuffered`]: naia_shared::ChannelMode::TickBuffered
    pub fn client_tick(&self) -> Option<Tick> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_sending_tick)
    }

    /// Returns the `GameInstant` corresponding to the client's current sending
    /// tick, or `None` if not connected.
    pub fn client_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_sending_instant)
    }

    /// Returns the server tick that the client is currently receiving, or
    /// `None` if not connected.
    ///
    /// This lags slightly behind the server's actual current tick due to
    /// network latency and the jitter buffer.
    pub fn server_tick(&self) -> Option<Tick> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_receiving_tick)
    }

    /// Returns the `GameInstant` corresponding to the current server-receive
    /// tick, or `None` if not connected.
    pub fn server_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_receiving_instant)
    }

    /// Converts a tick counter value to the corresponding `GameInstant`,
    /// or `None` if not connected.
    pub fn tick_to_instant(&self, tick: Tick) -> Option<GameInstant> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.tick_to_instant(tick));
        }
        None
    }

    /// Returns the duration of a single tick as configured in the protocol,
    /// or `None` if not connected.
    pub fn tick_duration(&self) -> Option<Duration> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.tick_duration());
        }
        None
    }

    // Interpolation ─────────────────────────────────────────────────────────

    /// Returns the interpolation fraction `[0.0, 1.0)` for the current frame
    /// within the client sending tick.
    ///
    /// Use this to lerp predicted entities between their state at the previous
    /// and current client ticks. Returns `None` if not connected.
    pub fn client_interpolation(&self) -> Option<f32> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.client_interpolation());
        }
        None
    }

    /// Returns the interpolation fraction `[0.0, 1.0)` for the current frame
    /// within the server receive tick.
    ///
    /// Use this to lerp authoritative server-replicated entities between their
    /// state at the previous and current server ticks. Returns `None` if not
    /// connected.
    pub fn server_interpolation(&self) -> Option<f32> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.server_interpolation());
        }
        None
    }

    // Diagnostics ───────────────────────────────────────────────────────────

    /// Returns the rolling-average outgoing bandwidth to the server
    /// (bytes/second).
    pub fn outgoing_bandwidth(&self) -> f32 {
        self.io.outgoing_bandwidth()
    }

    /// Returns the rolling-average incoming bandwidth from the server
    /// (bytes/second).
    pub fn incoming_bandwidth(&self) -> f32 {
        self.io.incoming_bandwidth()
    }

    /// Returns a snapshot of per-connection diagnostics.
    ///
    /// Returns `None` if not connected. Includes RTT (average in ms), jitter,
    /// packet-loss fraction, and send/recv bandwidth in kbps.
    pub fn connection_stats(&self) -> Option<ConnectionStats> {
        let conn = self.server_connection.as_ref()?;
        let rtt_ms = conn.time_manager.rtt();
        let jitter_ms = conn.time_manager.jitter();
        let packet_loss_pct = conn.base.packet_loss_pct();
        Some(ConnectionStats {
            rtt_ms,
            rtt_p50_ms: rtt_ms,
            rtt_p99_ms: conn.time_manager.rtt_p99_ms(),
            jitter_ms,
            packet_loss_pct,
            kbps_sent: self.io.outgoing_bandwidth(),
            kbps_recv: self.io.incoming_bandwidth(),
        })
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

    /// Despawns the entity from the replication layer without touching the
    /// world.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the ECS world has already removed the
    /// entity. Application code should despawn via the world, which triggers
    /// the adapter hook automatically.
    ///
    /// # Panics
    ///
    /// Panics if the entity is server-owned without delegation, or if the
    /// client does not hold `Granted` authority over a delegated entity.
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
                let is_delegated = self
                    .global_world_manager
                    .entity_is_delegated(&global_entity);
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
            connection
                .base
                .world_manager
                .despawn_entity_and_notify_server(&global_entity);
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
            let is_delegated = self
                .global_world_manager
                .entity_is_delegated(&global_entity);

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
    /// Returns the registered name of the component identified by `component_kind`; intended for debug logging.
    pub fn component_name(&self, component_kind: &ComponentKind) -> String {
        self.protocol.component_kinds.kind_to_name(component_kind)
    }

    /// Registers a component insertion with the replication layer without
    /// touching the world's component storage.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the component already exists in the
    /// ECS world. Application code should insert components via the world.
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // When authority is granted for a previously-remote delegated entity
        // (server calls give_authority while the entity is already in scope),
        // entity_complete_delegation has already registered this component in
        // the GlobalDiffHandler and set the Property to Delegated state.
        // Re-entering here would double-panic in both host_insert_component
        // and Property::enable_delegation.  Skip entirely.
        if self
            .global_world_manager
            .component_already_host_registered(&global_entity, &component_kind)
        {
            return;
        }

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

    /// Registers a component removal with the replication layer without
    /// touching the world's component storage.
    ///
    /// # Adapter use only
    ///
    /// The Bevy adapter calls this when the component has already been removed
    /// from the ECS world.
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Only relay through the outgoing pipeline if the entity is client-created
        // (i.e. tracked in the local/host world manager). For server-created entities
        // that the client merely holds authority over (e.g. delegated resources
        // removed by the server), the entity is not in the local world manager and
        // calling remove_component on it would panic.
        if let Some(connection) = &mut self.server_connection {
            if connection.base.world_manager.has_global_entity(&global_entity) {
                connection
                    .base
                    .world_manager
                    .remove_component(&global_entity, component_kind);
            }
        }

        // cleanup all other loose ends
        self.global_world_manager
            .host_remove_component(&global_entity, component_kind);
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
        } else if self
            .global_world_manager
            .entity_replication_config(global_entity)
            != Some(Publicity::Private)
        {
            panic!("Server can only publish Private entities");
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
        } else if self
            .global_world_manager
            .entity_replication_config(global_entity)
            != Some(Publicity::Public)
        {
            panic!("Server can only unpublish Public entities");
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
            (EntityAuthStatus::Requested, EntityAuthStatus::Granted)
            | (EntityAuthStatus::Denied, EntityAuthStatus::Granted)
            | (EntityAuthStatus::Available, EntityAuthStatus::Granted) => {
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
            (EntityAuthStatus::Granted, EntityAuthStatus::Available)
            | (EntityAuthStatus::Granted, EntityAuthStatus::Denied) => {
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
            // Available → Denied. Fires when another client (or the server)
            // takes authority for an entity that this client had been free to
            // request. Per contract `entity-delegation-15`: every transition
            // into Denied emits exactly one AuthDenied event so the
            // application can react (e.g. close a request UI, mark the
            // entity read-only).
            (EntityAuthStatus::Available, EntityAuthStatus::Denied) => {
                self.incoming_world_events.push_auth_deny(*world_entity);
            }
            (EntityAuthStatus::Denied, EntityAuthStatus::Available) => {
                // Release by someone else - emit reset
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            (EntityAuthStatus::Available, EntityAuthStatus::Available)
            | (EntityAuthStatus::Denied, EntityAuthStatus::Denied)
            | (EntityAuthStatus::Granted, EntityAuthStatus::Granted)
            | (EntityAuthStatus::Requested, EntityAuthStatus::Requested)
            | (EntityAuthStatus::Releasing, EntityAuthStatus::Releasing) => {
                // Idempotent — same-state transitions are no-ops. The grant/take/release
                // side effects (register, deregister, push_auth_grant, push_auth_reset)
                // already fired on the original transition into this state; receiving a
                // duplicate "you are still in state X" message must not double-fire them.
                // Granted→Granted in particular happens on the publication migration path
                // where MigrateResponse sets Granted (client.rs:2167) and an explicit
                // EntityUpdateAuth(Granted) follows.
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
        // Tick bandwidth monitors to clear expired packets
        self.io.tick_bandwidth_monitors();

        if self.server_connection.is_none() {
            self.maintain_handshake();
        }
        // Note: maintain_handshake may have just established the connection,
        // so we check again (not else) to immediately process any remaining
        // packets (e.g. entity replication data) that arrived in the same
        // transport batch as the final handshake response.
        if self.server_connection.is_some() {
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
                                self.incoming_world_events
                                    .push_rejection(&old_socket_addr, RejectReason::Auth);
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
                                *time_manager,
                                &self.global_world_manager,
                                self.client_config.jitter_buffer,
                                &self.protocol.component_kinds,
                            ));
                            self.on_connect();

                            let server_addr = self.server_address_unwrapped();
                            self.incoming_world_events.push_connection(&server_addr);

                            // Stop reading here — any remaining packets in
                            // the transport (e.g. Data packets with entity
                            // replication) must be processed through
                            // maintain_connection, not the handshake loop
                            // which silently discards non-handshake packets.
                            break;
                        }
                        Some(HandshakeResult::Rejected(reason)) => {
                            info!("Client: Received HandshakeResult::Rejected({:?})", reason);
                            let server_addr = self.server_address_unwrapped();
                            self.incoming_world_events
                                .push_rejection(&server_addr, reason);
                            self.disconnect_reset_connection();
                            break;
                        }
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

        // receive from socket
        loop {
            match self.io.recv_reader() {
                Ok(Some(mut reader)) => {
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
                        PacketType::Heartbeat | PacketType::Ping | PacketType::Pong => {
                            // these packet types are allowed when
                            // connection is established
                        }
                        PacketType::Handshake => {
                            // Server sent a handshake packet while connected -
                            // this should only be a Disconnect message
                            let Ok(handshake_header) = HandshakeHeader::de(&mut reader) else {
                                warn!("unable to parse handshake header from server");
                                continue;
                            };
                            if matches!(handshake_header, HandshakeHeader::Disconnect) {
                                info!("Received disconnect from server");
                                self.server_disconnect = true;
                            }
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
                                // Malformed pong: skip this sample. RTT estimation
                                // recovers on the next successful pong exchange.
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
            // Heartbeat send failure is not fatal: the server's connection
            // timeout will fire if heartbeats stop arriving persistently.
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

    fn disconnect_with_events<W: WorldMutType<E>>(&mut self, world: &mut W, reason: naia_shared::DisconnectReason) {
        let server_addr = self.server_address_unwrapped();

        self.incoming_world_events.clear();
        self.incoming_tick_events.clear();

        self.despawn_all_remote_entities(world);
        self.disconnect_reset_connection();

        self.incoming_world_events.push_disconnection(&server_addr, reason);
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
            self.protocol_id,
            self.client_config.send_handshake_interval,
            self.client_config.ping_interval,
            self.client_config.handshake_pings,
        ));

        self.manual_disconnect = false;
        let mut global_world_manager = GlobalWorldManager::new();
        global_world_manager.init_protocol_kind_count(self.protocol.component_kinds.kind_count());
        self.global_world_manager = global_world_manager;
    }

    fn server_address_unwrapped(&self) -> SocketAddr {
        // NOTE: may panic if the connection is not yet established!
        self.io.server_addr().expect("connection not established!")
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_diagnostic(
        &self,
        remote_entity: &naia_shared::RemoteEntity,
    ) -> Option<(
        naia_shared::EntityChannelState,
        (
            naia_shared::SubCommandId,
            usize,
            Option<naia_shared::SubCommandId>,
            usize,
        ),
    )> {
        let Some(connection) = self.server_connection.as_ref() else {
            return None;
        };
        connection
            .base
            .world_manager
            .debug_remote_channel_diagnostic(remote_entity)
    }

    #[cfg(feature = "e2e_debug")]
    pub fn debug_remote_channel_snapshot(
        &self,
        remote_entity: &naia_shared::RemoteEntity,
    ) -> Option<(
        naia_shared::EntityChannelState,
        Option<naia_shared::MessageIndex>,
        usize,
        Option<(naia_shared::MessageIndex, naia_shared::EntityMessageType)>,
        Option<naia_shared::MessageIndex>,
    )> {
        let Some(connection) = self.server_connection.as_ref() else {
            return None;
        };
        connection
            .base
            .world_manager
            .debug_remote_channel_snapshot(remote_entity)
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
                    // Resource registry maintenance: if this entity was
                    // a resource entity, clear the registry record so
                    // future has_resource::<R>() calls return false.
                    self.resource_registry.remove_by_entity(&global_entity);
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
                    // Resource registry maintenance: if the inserted
                    // component is a Replicated Resource kind, record
                    // the (TypeId, GlobalEntity) mapping so the bevy
                    // adapter's mirror system + has_resource::<R>()
                    // lookups work O(1).
                    if self.protocol.resource_kinds.is_resource(&component_kind) {
                        let type_id: std::any::TypeId = component_kind.into();
                        let _ = self.resource_registry.insert_raw(type_id, global_entity);
                    }
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
                            global_entity,
                            delegated_at_entry
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
                        let old_entity = OwnedLocalEntity::Host { id: old_host_entity.value(), is_static: false };
                        let new_entity = new_remote_entity.copy_to_owned();
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

                    // Register the entity with the client's auth handler
                    // before completing delegation. Without this, the
                    // `entity_complete_delegation` → `world.entity_enable_delegation`
                    // → `component_enable_delegation` chain panics in
                    // `host_auth_handler::get_accessor` because the
                    // owning-client (A's) MigrateResponse path skips the
                    // `entity_register_auth_for_delegation` call that the
                    // EnableDelegation event path uses for non-owners.
                    // Both paths must produce the same registered state
                    // before components are flipped to delegated mode.
                    //
                    // Idempotent guard: some flows (e.g. the publication
                    // migration path tested by [entity-publication-08])
                    // register the entity earlier via the EnableDelegation
                    // event before MigrateResponse arrives — calling
                    // `register_entity` again would panic.
                    if self
                        .global_world_manager
                        .entity_authority_status(&global_entity)
                        .is_none()
                    {
                        self.global_world_manager
                            .entity_register_auth_for_delegation(&global_entity);
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

/// The lifecycle state of the client's connection to the server.
///
/// Retrieved via [`Client::connection_status`].
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// No socket is open; [`connect`](Client::connect) has not been called.
    Disconnected,
    /// The socket is open and the handshake is in progress.
    Connecting,
    /// The handshake is complete and the connection is active.
    Connected,
    /// [`disconnect`](Client::disconnect) has been called; awaiting confirmation.
    Disconnecting,
}

impl ConnectionStatus {
    /// Returns `true` if the client is fully disconnected.
    pub fn is_disconnected(&self) -> bool {
        self == &ConnectionStatus::Disconnected
    }

    /// Returns `true` if the handshake is in progress.
    pub fn is_connecting(&self) -> bool {
        self == &ConnectionStatus::Connecting
    }

    /// Returns `true` if the connection is active.
    pub fn is_connected(&self) -> bool {
        self == &ConnectionStatus::Connected
    }

    /// Returns `true` if the client is tearing down an active connection.
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
