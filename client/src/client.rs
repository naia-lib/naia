use std::{any::Any, collections::VecDeque, hash::Hash, net::SocketAddr, time::Duration};

use log::{debug, info, warn};

use naia_shared::{
    handshake::{HandshakeHeader, RejectReason},
    AuthorityError, BitReader, BitWriter, CancelDisposition, Channel, ChannelKind, ChannelMode,
    ComponentKind, ConnectionStats, DisconnectReason, EntityAndGlobalEntityConverter,
    EntityAuthStatus, EntityDoesNotExistError, EntityEvent, EntityPriorityMut, EntityPriorityRef,
    FakeEntityConverter, GameInstant, GlobalEntity, GlobalEntityMap, GlobalEntitySpawner,
    GlobalRequestId, GlobalResponseId, GlobalWorldManagerType, HostType, Instant,
    LocalEntityAndGlobalEntityConverter, Message, MessageContainer, OwnedLocalEntity, PacketType,
    Protocol, ProtocolId, Replicate, ReplicatedComponent, Request, RequestPoll, Response,
    ResponseReceiveKey, ResponseSendKey, Serde, SharedGlobalWorldManager, SocketConfig,
    StandardHeader, Tick, Timer, UserPriorityState, WorldMutType, WorldRefType,
    PROTOCOL_MISMATCH_STATUS,
};

use super::{
    client_config::ClientConfig, error::NaiaClientError, world_events::Events, JitterBufferType,
};
use crate::{
    connection::{base_time_manager::BaseTimeManager, connection::Connection, io::Io},
    handshake::{HandshakeManager, HandshakeResult, Handshaker},
    request::SlotPoll,
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
    /// Set when the server's disconnect packet named a reason, with the
    /// serialized message it carried (naia-lib/naia#10).
    server_disconnect_details: Option<(naia_shared::DisconnectReason, Option<Vec<u8>>)>,
    server_disconnect: bool,
    /// Bounds the handshake: fires when an attempt runs longer than the
    /// link-silence deadline without connecting. Reset at every attempt
    /// start (`connect`) and every attempt teardown (`reset_attempt_state`);
    /// consulted only while the handshake is in flight, never once
    /// `server_connection` exists.
    handshake_timeout: Timer,
    /// Counts handshake-packet send failures within the current attempt so
    /// the first success can report how lossy the path was. Reset with the
    /// attempt, alongside the two first-event flags below. Connection-phase
    /// logging for the handshake diagnosis (Usher 42357).
    handshake_failed_sends: u32,
    /// Whether the first-success line has fired for the current attempt;
    /// reset with the attempt so each dial reports its own handshake once.
    handshake_first_send_logged: bool,
    /// Whether the first inbound datagram of the current attempt has been
    /// logged; same attempt scope as the send counters.
    handshake_first_inbound_logged: bool,
    /// Post-Connected liveness watch (Usher 42587 fork (a) markers). Set at
    /// handshake Connected; counts inbound data + outbound keepalives and
    /// fires one-shot warn summaries at +30s/+60s so a single probe run
    /// separates "client stopped sending" from "sent but lost".
    post_connect_watch: Option<PostConnectWatch>,
    waitlist_messages: VecDeque<(ChannelKind, Box<dyn Message>)>,
    // World
    global_world_manager: GlobalWorldManager,
    global_entity_map: GlobalEntityMap<E>,
    // Events
    incoming_world_events: Events<E>,
    /// Fail-closed delegation error slot (card 27945). Without the
    /// `entity_delegation` feature this records
    /// `AuthorityError::DelegationDisabled` when a delegation wire command
    /// arrives; always `None` with the feature on. See
    /// [`Client::take_authority_error`].
    authority_error: Option<AuthorityError>,
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

/// Post-Connected liveness counters for the Usher 42587 fork-(a) markers.
///
/// Lives on the client from handshake Connected until disconnect. All
/// fields are marker-only: they never gate behavior.
struct PostConnectWatch {
    connected_at: Instant,
    data_rx: u64,
    data_applied: u64,
    keepalive_sent: u64,
    last_keepalive_ok: Option<bool>,
    first_data_warned: bool,
    summary_30_warned: bool,
    summary_60_warned: bool,
}

impl PostConnectWatch {
    fn new() -> Self {
        Self {
            connected_at: Instant::now(),
            data_rx: 0,
            data_applied: 0,
            keepalive_sent: 0,
            last_keepalive_ok: None,
            first_data_warned: false,
            summary_30_warned: false,
            summary_60_warned: false,
        }
    }

    /// One-shot summary schedule: returns (fire_30s, fire_60s) for the given
    /// elapsed-seconds count, setting the fired flags so each summary fires
    /// exactly once. Pure takes-elapsed-seconds form so tests can pin the
    /// schedule without waiting out a wall clock.
    fn summaries_due(&mut self, elapsed_secs: u64) -> (bool, bool) {
        let fire_30 = elapsed_secs >= 30 && !self.summary_30_warned;
        let fire_60 = elapsed_secs >= 60 && !self.summary_60_warned;
        if fire_30 {
            self.summary_30_warned = true;
        }
        if fire_60 {
            self.summary_60_warned = true;
        }
        (fire_30, fire_60)
    }
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
    #[must_use]
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
            server_disconnect_details: None,
            handshake_timeout: Timer::new(client_config.connection.disconnection_timeout_duration),
            handshake_failed_sends: 0,
            handshake_first_send_logged: false,
            handshake_first_inbound_logged: false,
            waitlist_messages: VecDeque::new(),
            // World
            global_world_manager,
            global_entity_map: GlobalEntityMap::new(),
            // Events
            incoming_world_events: Events::new(),
            // Fail-closed delegation error slot (card 27945). Without the
            // `entity_delegation` feature, delegation wire commands are
            // recorded here instead of applied; `None` in ON builds.
            authority_error: None,
            incoming_tick_events: TickEvents::new(),
            priority: UserPriorityState::new(),
            resource_registry: naia_shared::ResourceRegistry::new(),
            post_connect_watch: None,
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
    /// `AuthEvent` in its connection handler.
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
    /// until a [`ConnectEvent`] arrives.
    ///
    /// # Panics
    ///
    /// Panics if the client has already initiated a connection. Check
    /// [`connection_status`](Client::connection_status) before calling.
    ///
    /// [`ConnectEvent`]: crate::ConnectEvent
    pub fn connect<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        assert!(self.is_disconnected(), "Client has already initiated a connection, cannot initiate a new one. TIP: Check client.is_disconnected() before calling client.connect()");

        // Start the handshake deadline: a fresh attempt gets the full
        // link-silence window. (Without this, a client constructed long
        // before its first connect would time out immediately.)
        self.handshake_timeout.reset();

        // The fingerprint goes on every one of these branches, including the
        // no-auth one: `require_auth = false` still means the two ends have to
        // agree on the protocol before either decodes the other's packets.
        let protocol_id = self.protocol_id;

        if let Some(auth_bytes) = &self.auth_message {
            if let Some(auth_headers) = &self.auth_headers {
                // connect with auth & headers
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) = boxed_socket
                    .connect_with_auth_and_headers(
                        protocol_id,
                        auth_bytes.clone(),
                        auth_headers.clone(),
                    );
                self.io.load(id_receiver, packet_sender, packet_receiver);
            } else {
                // connect with auth
                let boxed_socket: Box<dyn Socket> = socket.into();
                let (id_receiver, packet_sender, packet_receiver) =
                    boxed_socket.connect_with_auth(protocol_id, auth_bytes.clone());
                self.io.load(id_receiver, packet_sender, packet_receiver);
            }
        } else if let Some(auth_headers) = &self.auth_headers {
            // connect with auth headers
            let boxed_socket: Box<dyn Socket> = socket.into();
            let (id_receiver, packet_sender, packet_receiver) =
                boxed_socket.connect_with_auth_headers(protocol_id, auth_headers.clone());
            self.io.load(id_receiver, packet_sender, packet_receiver);
        } else {
            // connect without auth
            let boxed_socket: Box<dyn Socket> = socket.into();
            let (id_receiver, packet_sender, packet_receiver) = boxed_socket.connect(protocol_id);
            self.io.load(id_receiver, packet_sender, packet_receiver);
        }
    }

    /// Returns the client's current connection lifecycle state.
    ///
    /// Transitions: `Disconnected` → `Connecting` (after
    /// [`connect`](Client::connect)) → `Connected` (after handshake) →
    /// `Disconnecting` (after [`disconnect`](Client::disconnect)) →
    /// `Disconnected`.
    #[must_use]
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
    /// then begins the disconnection process. A [`DisconnectEvent`] is
    /// emitted on the next [`take_world_events`](Client::take_world_events)
    /// call.
    ///
    /// # Panics
    ///
    /// Panics if the client is not currently connected.
    ///
    /// [`DisconnectEvent`]: crate::DisconnectEvent
    pub fn disconnect(&mut self) {
        assert!(
            self.is_connected(),
            "Trying to disconnect Client which is not connected yet!"
        );

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

    /// Cancels a pending (not yet established) connection attempt, returning
    /// the client to `Disconnected` so a fresh [`connect`](Client::connect)
    /// starts cleanly instead of panicking on the loaded socket.
    ///
    /// DWO-2: an in-match grace expiry must be able to abandon a stuck
    /// handshake and renew the attempt on the same client object. Safe to
    /// call in any state — a live connection is left untouched (orderly
    /// teardown stays [`disconnect`](Client::disconnect)'s job) and an
    /// idle client stays idle. Emits no events and drops no entities:
    /// nothing was ever established.
    pub fn cancel_connect(&mut self) {
        if self.server_connection.is_some() {
            // Established connection: not a pending attempt, leave it alone.
            return;
        }
        self.reset_attempt_state();
    }

    /// Returns the socket configuration from the protocol.
    #[must_use]
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
    /// for the next [`take_world_events`](Client::take_world_events) call.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        // all other operations
        if self.is_disconnecting() {
            let (reason, payload) = Self::resolve_disconnect_reason(
                self.server_disconnect_details.take(),
                self.manual_disconnect,
                self.server_disconnect,
            );
            let message = payload.and_then(|bytes| self.decode_server_message(&bytes));
            self.disconnect_with_events(&mut world, reason, message);
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

        // Cap the receiving tick at the latest delivered server tick — a universal
        // correctness invariant: the confirmed timeline must never reconstruct a tick
        // whose authoritative state hasn't been received (deterministic; aligns with
        // the netcode contract that confirmed[T] is a bit-identical reconstruction of
        // server[T], which requires T's data). In real-net production the jitter +
        // latency margin already keeps the clock estimate behind delivery so this
        // rarely fires; in deterministic test_time it actively prevents the race
        // where the clock estimate reaches T before T's packet arrives.
        let receiving_tick_ceiling = connection.last_received_server_tick();

        let (receiving_tick_happened, sending_tick_happened) = connection
            .time_manager
            .collect_ticks(now, receiving_tick_ceiling);

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
                match self.io.send_packet(outgoing_packet) {
                    Ok(()) => {
                        if !self.handshake_first_send_logged {
                            self.handshake_first_send_logged = true;
                            warn!(
                                "naia: Client: first handshake packet sent to Server \
                                 (failed sends before it in this attempt: {})",
                                self.handshake_failed_sends
                            );
                        }
                        self.handshake_failed_sends = 0;
                    }
                    Err(_) => {
                        self.handshake_failed_sends += 1;
                        // Single handshake send failure is not fatal: the handshake
                        // manager retries on the next tick until the server responds.
                        warn!("Client Error: Cannot send handshake packet to Server");
                    }
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
                .send
                .world_manager
                .entity_converter_mut(&self.global_world_manager);
            let message = MessageContainer::new(message_box);
            let accepted = connection.base.send.message_manager.send_message(
                &self.protocol.message_kinds,
                &mut converter,
                channel_kind,
                message,
            );
            if !accepted {
                return Err(NaiaClientError::MessageQueueFull);
            }
        } else {
            // No connection: the waitlist IS the queue, so the channel's
            // max_queue_depth applies here exactly as it does on the live
            // path. Otherwise pre-connect submits bypass the documented
            // MessageQueueFull backpressure without bound.
            if let ChannelMode::UnorderedReliable(settings)
            | ChannelMode::SequencedReliable(settings)
            | ChannelMode::OrderedReliable(settings) = &channel_settings.mode
            {
                if let Some(max) = settings.max_queue_depth {
                    let queued = self
                        .waitlist_messages
                        .iter()
                        .filter(|(kind, _)| kind == channel_kind)
                        .count();
                    if queued >= max {
                        return Err(NaiaClientError::MessageQueueFull);
                    }
                }
            }
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

        assert!(
            channel_settings.can_request_and_respond(),
            "Requests can only be sent over Bidirectional, Reliable Channels"
        );

        let Some(connection) = &mut self.server_connection else {
            warn!("currently not connected to server");
            return Err(NaiaClientError::NotConnected);
        };
        let mut converter = connection
            .base
            .send
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        // H3: the nonce supply is checked — exhaustion retires the
        // connection rather than aliasing a live nonce, so a spent supply
        // is a typed backpressure error, not a panic. The nonce names the
        // exchange on the wire (envelope cutover, codec grammar 2).
        let (request_id, nonce) = connection
            .global_request_manager
            .create_request_id()
            .map_err(|_| NaiaClientError::RequestNonceExhausted)?;
        let message = MessageContainer::new(request_box);
        if !connection.base.send.message_manager.send_request(
            &self.protocol.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            nonce,
            message,
        ) {
            // Queue-depth cap reached: nothing was enqueued. Report it rather than
            // handing back an id whose response will never arrive.
            return Err(NaiaClientError::MessageQueueFull);
        }

        Ok(request_id)
    }

    /// Sends a response to the server's request.
    ///
    /// `response_key` is obtained from the [`RequestEvent`] that delivered
    /// the server's original request. Returns `true` on success; `false` if
    /// the key is no longer valid (e.g. the connection was dropped).
    ///
    /// [`RequestEvent`]: crate::RequestEvent
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
        // Peek, don't consume: if the enqueue is refused below, the mapping must
        // survive so the caller can retry with the same key. H3: the kept
        // wire nonce is echoed so the requester resolves by (id, nonce).
        let Some((channel_kind, local_response_id, nonce)) = connection
            .global_response_manager
            .peek_response_id(response_id)
        else {
            return false;
        };
        let mut converter = connection
            .base
            .send
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        let response = MessageContainer::new(response_box);
        let accepted = connection.base.send.message_manager.send_response(
            &self.protocol.message_kinds,
            &mut converter,
            &channel_kind,
            local_response_id,
            nonce,
            response,
        );
        if accepted {
            connection
                .global_response_manager
                .destroy_response_id(response_id);
        }
        accepted
    }

    /// Returns `true` if a response to the given request has arrived.
    ///
    /// Non-destructive — does not consume the response. Call
    /// [`receive_response`](Client::receive_response) to retrieve and consume
    /// it.
    #[must_use]
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

    /// Cancels a pending request (H3 request abandonment).
    ///
    /// Removes the routing entry and marks the exchange's nonce abandoned,
    /// so a late response drops instead of resurrecting the slot. Returns
    /// [`CancelDisposition::UnknownKey`] when no entry exists — without a
    /// connection, or for an already completed, cancelled, or never-sent
    /// key.
    pub fn cancel_request<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> CancelDisposition {
        let Some(connection) = &mut self.server_connection else {
            return CancelDisposition::UnknownKey;
        };
        connection
            .global_request_manager
            .cancel_request(&response_key.request_id())
    }

    /// Non-destructive poll of a pending request (H3 request abandonment).
    ///
    /// [`RequestPoll::Response`] carries the decoded reply without
    /// consuming it — call
    /// [`receive_response`](Client::receive_response) to take it.
    /// [`RequestPoll::Abandoned`] means no reply will ever arrive: the
    /// request was cancelled or the key names nothing live.
    pub fn poll_request<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> RequestPoll<S> {
        let Some(connection) = &mut self.server_connection else {
            return RequestPoll::Abandoned;
        };
        match connection
            .global_request_manager
            .poll_slot(&response_key.request_id())
        {
            SlotPoll::Pending => RequestPoll::Pending,
            SlotPoll::Abandoned => RequestPoll::Abandoned,
            SlotPoll::Ready => {
                let container = connection
                    .global_request_manager
                    .peek_request(&response_key.request_id())
                    .expect("Ready slot holds its response until taken");
                let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
                    .ok()
                    .map(|boxed_s| *boxed_s)
                    .unwrap();
                RequestPoll::Response(response)
            }
        }
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

        assert!(
            channel_settings.can_send_to_server(),
            "Cannot send message to Server on this Channel"
        );

        assert!(channel_settings.tick_buffered(), "Can only use `Client.send_tick_buffer_message()` on a Channel that is configured for it.");

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
            self.global_world_manager
                .host_spawn_static_entity(&global_entity);
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
        connection.base.send.world_manager.host_init_entity(
            global_entity,
            component_kinds,
            &self.protocol.component_kinds,
            is_static,
        );
    }

    // Replicated Resources (client-side mirror) ─────────────────────────────
    // Populated when the remote-apply path delivers an InsertComponent for a
    // resource kind. Clears on Despawn. The Bevy adapter consumes this to
    // drive the Bevy-Resource mirror (see adapters/bevy/client/src/resource_sync).

    /// Returns `true` if the client has a server-replicated resource of type
    /// `R` currently in scope.
    #[must_use]
    pub fn has_resource<R: 'static>(&self) -> bool {
        self.resource_registry.entity_for::<R>().is_some()
    }

    /// O(1): the world-entity carrying resource `R` on this client,
    /// or `None` if not currently in scope.
    #[must_use]
    pub fn resource_entity<R: 'static>(&self) -> Option<E> {
        let global_entity = self.resource_registry.entity_for::<R>()?;
        self.global_entity_map
            .global_entity_to_entity(global_entity)
            .ok()
    }

    /// True iff `world_entity` is the entity carrying any Replicated
    /// Resource currently in scope on this client.
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return false;
        };
        self.resource_registry.is_resource_entity(global_entity)
    }

    /// Number of currently-mirrored Replicated Resources.
    #[must_use]
    pub fn resources_count(&self) -> usize {
        self.resource_registry.len()
    }

    /// Iterate over the world-entities of all currently-mirrored resources.
    #[must_use]
    pub fn resource_entities(&self) -> Vec<E> {
        let mut out = Vec::with_capacity(self.resource_registry.len());
        for global_entity in self.resource_registry.entities() {
            if let Ok(e) = self
                .global_entity_map
                .global_entity_to_entity(*global_entity)
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
        self.global_world_manager
            .mark_entity_as_static(&global_entity);
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
        assert!(self.global_world_manager.has_entity(&global_entity), "Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Client, before configuring replication.");
        let entity_owner = self
            .global_world_manager
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned = entity_owner.is_server();
        assert!(
            !server_owned,
            "Client cannot configure replication strategy of Server-owned Entities."
        );
        let client_owned = entity_owner.is_client();
        assert!(
            client_owned,
            "Client cannot configure replication strategy of Entities it does not own."
        );
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

        self.global_world_manager
            .entity_authority_status(&global_entity)
    }

    /// Takes a pending fail-closed authority error, if any (card 27945).
    ///
    /// Without the `entity_delegation` feature, delegation wire commands are
    /// not applied; each one records `AuthorityError::DelegationDisabled`
    /// here (and logs it) instead. Returns `None` — and, with the feature on,
    /// always returns `None` — when no such command has arrived since the
    /// last take.
    pub fn take_authority_error(&mut self) -> Option<AuthorityError> {
        self.authority_error.take()
    }

    /// Records a fail-closed delegation refusal (card 27945). Without the
    /// `entity_delegation` feature this is the single funnel for delegation
    /// wire commands that reach the client: the command is NOT applied, the
    /// named error is logged and stored for [`Client::take_authority_error`]
    /// — never silently dropped, never panicked.
    #[cfg(not(feature = "entity_delegation"))]
    fn record_delegation_disabled(
        &mut self,
        command: &'static str,
        entity: &naia_shared::GlobalEntity,
    ) {
        log::error!(
            "entity delegation is disabled in this build; refusing {command} for entity {entity:?}"
        );
        self.authority_error
            .get_or_insert(AuthorityError::DelegationDisabled);
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
    /// [`EntityAuthGrantedEvent`]: crate::EntityAuthGrantedEvent
    /// [`EntityAuthDeniedEvent`]: crate::EntityAuthDeniedEvent
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
                .send
                .world_manager
                .remote_send_request_auth(global_entity);
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
                .send
                .world_manager
                .remote_send_release_auth(global_entity);
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
    #[must_use]
    pub fn rtt(&self) -> f32 {
        self.server_connection
            .as_ref()
            .map_or(0.0, |conn| conn.time_manager.rtt() / 1000.0)
    }

    /// Returns the rolling-average jitter (seconds) measured for the server
    /// connection.
    ///
    /// Returns `0.0` if the connection has not been established yet.
    #[must_use]
    pub fn jitter(&self) -> f32 {
        self.server_connection
            .as_ref()
            .map_or(0.0, |conn| conn.time_manager.jitter() / 1000.0)
    }

    // Ticks ─────────────────────────────────────────────────────────────────

    /// Returns the client's current sending tick, or `None` if not connected.
    ///
    /// This is the tick at which the client is currently sending — use it to
    /// stamp [`TickBuffered`] messages for prediction.
    ///
    /// [`TickBuffered`]: naia_shared::ChannelMode::TickBuffered
    #[must_use]
    pub fn client_tick(&self) -> Option<Tick> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_sending_tick)
    }

    /// Returns the `GameInstant` corresponding to the client's current sending
    /// tick, or `None` if not connected.
    #[must_use]
    pub fn client_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_sending_instant)
    }

    /// Returns the server tick that the client is currently receiving, or
    /// `None` if not connected.
    ///
    /// This lags slightly behind the server's actual current tick due to
    /// network latency and the jitter buffer.
    #[must_use]
    pub fn server_tick(&self) -> Option<Tick> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_receiving_tick)
    }

    /// Returns the `GameInstant` corresponding to the current server-receive
    /// tick, or `None` if not connected.
    #[must_use]
    pub fn server_instant(&self) -> Option<GameInstant> {
        let connection = self.server_connection.as_ref()?;
        Some(connection.time_manager.client_receiving_instant)
    }

    /// Converts a tick counter value to the corresponding `GameInstant`,
    /// or `None` if not connected.
    #[must_use]
    pub fn tick_to_instant(&self, tick: Tick) -> Option<GameInstant> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.tick_to_instant(tick));
        }
        None
    }

    /// Returns the duration of a single tick as configured in the protocol,
    /// or `None` if not connected.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn server_interpolation(&self) -> Option<f32> {
        if let Some(connection) = &self.server_connection {
            return Some(connection.time_manager.server_interpolation());
        }
        None
    }

    // Diagnostics ───────────────────────────────────────────────────────────

    /// Returns the rolling-average outgoing bandwidth to the server
    /// (bytes/second).
    #[must_use]
    pub fn outgoing_bandwidth(&self) -> f32 {
        self.io.outgoing_bandwidth()
    }

    /// Returns the rolling-average incoming bandwidth from the server
    /// (bytes/second).
    #[must_use]
    pub fn incoming_bandwidth(&self) -> f32 {
        self.io.incoming_bandwidth()
    }

    /// Returns a snapshot of per-connection diagnostics.
    ///
    /// Returns `None` if not connected. Includes RTT (average in ms), jitter,
    /// packet-loss fraction, and send/recv bandwidth in kbps.
    #[must_use]
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
        assert!(
            world.has_entity(entity),
            "attempted to de-spawn nonexistent entity"
        );

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
                assert!(is_delegated, "attempting to despawn entity that is not yet delegated. Delegation needs some time to be confirmed by the Server, so check that a despawn is possible by calling `commands.entity(..).replication_config(..).is_delegated()` first.");
                // For HOST (client-origin) delegated entities, the host channel
                // sends the despawn to the server unconditionally — no Granted check
                // needed. `despawn_entity_and_notify_server` → `despawn_entity` →
                // `host.send_command(Despawn)` works regardless of auth status.
                // Server-initiated despawns never reach here: those entities are
                // deregistered from `global_entity_map` in ProcessPackets, causing
                // an early return at line 1336.
            }
        } else {
            panic!("attempting to despawn entity that has no owner");
        }

        if let Some(connection) = &mut self.server_connection {
            connection
                .base
                .send
                .world_manager
                .despawn_entity_and_notify_server(global_entity);
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
        assert!(
            world.has_entity(entity),
            "attempted to add component to non-existent entity"
        );

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

        if world.has_component_of_kind(entity, component_kind) {
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
    #[must_use]
    pub fn component_name(&self, component_kind: &ComponentKind) -> String {
        self.protocol.component_kinds.kind_to_name(*component_kind)
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
                .send
                .world_manager
                .has_global_entity(global_entity)
            {
                connection
                    .base
                    .send
                    .world_manager
                    .insert_component(global_entity, component_kind);
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
                .get_entity_auth_accessor(global_entity);
            component.enable_delegation(&accessor, None);
        }
    }

    /// Drains the buffered UPDATES for component `R`, applies each to `world` in
    /// tick order, and returns the resulting value per update as `(Tick, E, R)`.
    /// Drained entries are removed from the receive buffer so the later full apply
    /// ([`Self::process_all_packets`]) does not re-apply them; all other buffered
    /// updates and inserts are left for that later apply.
    ///
    /// This exposes the gap naia maintains between **decode** (packets are read and
    /// buffered in `take_tick_events`, before the `HandleTickEvents` set) and **apply**
    /// (`process_all_packets`, after). It lets a client read a remote *input*
    /// component's PER-TICK history (e.g. a remote avatar's command at each tick it
    /// changed) BEFORE ticking its simulation — so a client-confirmed re-simulation
    /// can re-derive every tick in a catch-up batch with that tick's own input —
    /// then reconcile the remaining *state* afterward (the input-early / state-late
    /// split). See [`take_received_updates_of_kind`](Client::take_received_updates_of_kind).
    pub fn take_received_updates_of_kind<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        mut world: W,
    ) -> Vec<(Tick, E, R)> {
        let Self {
            server_connection,
            global_entity_map,
            ..
        } = self;
        let Some(connection) = server_connection.as_mut() else {
            return Vec::new();
        };
        connection
            .base
            .send
            .world_manager
            .take_received_updates_of_kind::<E, W, R>(&*global_entity_map, &mut world)
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
            if connection
                .base
                .send
                .world_manager
                .has_global_entity(global_entity)
            {
                connection
                    .base
                    .send
                    .world_manager
                    .remove_component(global_entity, *component_kind);
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
                .send
                .world_manager
                .send_publish(HostType::Client, *global_entity);
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
                .send
                .world_manager
                .send_unpublish(HostType::Client, *global_entity);
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
            connection.base.send.world_manager.send_enable_delegation(
                HostType::Client,
                true,
                *global_entity,
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

    #[cfg_attr(not(feature = "entity_delegation"), allow(dead_code))]
    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_is_origin: bool,
    ) {
        info!("client.entity_disable_delegation");
        assert!(
            !client_is_origin,
            "Cannot disable delegation from Client. Server owns all delegated Entities."
        );

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
            connection
                .base
                .send
                .world_manager
                .despawn_entity(*global_entity);
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
                .send
                .world_manager
                .get_remote_entity_auth_status(*global_entity);

            // Only sync if entity exists as RemoteEntity (i.e., migration completed)
            if channel_status_before.is_some() {
                connection
                    .base
                    .send
                    .world_manager
                    .remote_receive_set_auth(*global_entity, new_auth_status);
            } else {
                warn!(
                    "Entity {global_entity:?} not yet migrated to RemoteEntity - channel sync skipped"
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
            (
                EntityAuthStatus::Requested
                | EntityAuthStatus::Denied
                | EntityAuthStatus::Available,
                EntityAuthStatus::Granted,
            ) => {
                // Register and emit grant event
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .send
                    .world_manager
                    .register_authed_entity(&self.global_world_manager, *global_entity);
                self.incoming_world_events.push_auth_grant(*world_entity);
                #[cfg(feature = "e2e_debug")]
                {
                    use crate::counters::CLIENT_EMIT_AUTH_GRANTED_EVENT;
                    use std::sync::atomic::Ordering;
                    CLIENT_EMIT_AUTH_GRANTED_EVENT.fetch_add(1, Ordering::Relaxed);
                }
            }
            // Lose authority (must deregister and emit reset)
            (EntityAuthStatus::Granted, EntityAuthStatus::Available | EntityAuthStatus::Denied) => {
                // Deregister and emit reset event
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .send
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, *global_entity);
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            // Request denied (Requested -> Denied or Requested -> Available)
            // Available case: server made entity Available (e.g. cascade despawn) while a
            // request was in-flight — client never held authority, nothing to deregister.
            (
                EntityAuthStatus::Requested,
                EntityAuthStatus::Denied | EntityAuthStatus::Available,
            ) => {
                // Emit denied event, but do NOT deregister (never had authority)
                self.incoming_world_events.push_auth_deny(*world_entity);
            }
            // Release flow
            (EntityAuthStatus::Releasing, EntityAuthStatus::Available) => {
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .send
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, *global_entity);
                self.incoming_world_events.push_auth_reset(*world_entity);
            }
            (EntityAuthStatus::Releasing, EntityAuthStatus::Denied) => {
                // Server takeover during release
                self.server_connection
                    .as_mut()
                    .unwrap()
                    .base
                    .send
                    .world_manager
                    .deregister_authed_entity(&self.global_world_manager, *global_entity);
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
                    "-- Entity {global_entity:?} updated authority, not handled -- {old_auth_status:?} -> {new_auth_status:?}"
                );
            }
        }
    }

    // Private methods

    fn check_client_authoritative_allowed(&self) {
        assert!(self.protocol.client_authoritative_entities, "Cannot perform this operation: Client Authoritative Entities are not enabled! Enable them in the Protocol, with the `enable_client_authoritative_entities() method, and note that if you do enable them, to make sure you handle all Spawn/Insert/Update events in the Server, as this may be an attack vector.");
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

        // Bounded handshake (Usher 38421): an attempt that hears nothing for
        // a full link-silence window must fail loudly instead of
        // retransmitting forever. At the deadline the client reports
        // `AuthTimeout` on the same disconnection event path the
        // established link uses, then tears the attempt down (which also
        // disarms the timer, so exactly one event fires). The deadline
        // measures SILENCE, not time since dial: any inbound packet
        // re-arms it below, mirroring `mark_heard` on the established
        // path — otherwise a healthy-but-slow handshake under a short
        // silence window would report AuthTimeout mid-connect. The
        // address is attached when one is known; while still `Finding`
        // there is nothing honest to attribute, but the state still
        // resets so the consumer's retry loop re-engages.
        if self.handshake_timeout.ringing() {
            match self.io.server_addr() {
                Ok(server_addr) => {
                    self.incoming_world_events.push_disconnection(
                        &server_addr,
                        DisconnectReason::AuthTimeout,
                        None,
                    );
                }
                Err(_) => {
                    warn!(
                        "Client: handshake timed out with the server address \
                         still unknown; reporting Disconnected without an event"
                    );
                }
            }
            // Same teardown as `cancel_connect`: nothing was ever
            // established, so no world state is touched — the reset drops
            // the socket, the attempt flags, and the rung timer together.
            self.reset_attempt_state();
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
                IdentityReceiverResult::ErrorResponseCode(code, reject_payload) => {
                    // Capture the data address BEFORE resetting I/O: a pre-auth
                    // HTTP/auth rejection lands while the address is still
                    // unknown (Finding), and after the reset below there is
                    // nothing left to ask. `None` is reported honestly -- never
                    // a manufactured address, never a degraded generic error.
                    let old_socket_addr_result = self.io.server_addr();

                    // reset connection
                    self.io = Io::new(
                        &self.client_config.connection.bandwidth_measure_duration,
                        &self.protocol.compression,
                    );

                    let old_socket_addr = old_socket_addr_result.as_ref().ok().copied();

                    if code == PROTOCOL_MISMATCH_STATUS {
                        // The server refused before application auth: the peer
                        // runs a different protocol. Exactly one
                        // `RejectEvent(ProtocolMismatch)`, no message -- the
                        // mismatch response is always payload-free and carries
                        // neither fingerprint. No retry, no downgrade, no
                        // ConnectEvent, no extra ErrorEvent.
                        self.incoming_world_events.push_rejection(
                            old_socket_addr,
                            RejectReason::ProtocolMismatch,
                            None,
                        );
                    } else if code == 401 {
                        // The server may have sent a message explaining the
                        // rejection (naia-lib/naia#133). A payload we cannot
                        // decode is a protocol mismatch on the reject message
                        // itself -- report the rejection anyway, since that is
                        // the part the application must act on. The 401 rides
                        // the same capture path: the learned address when one
                        // is known, None while still Finding.
                        let reject_message = reject_payload.and_then(|bytes| {
                            let mut reader = BitReader::new(&bytes);
                            if let Ok(container) = self
                                .protocol
                                .message_kinds
                                .read(&mut reader, &FakeEntityConverter)
                            {
                                Some(container)
                            } else {
                                warn!(
                                    "Server sent a rejection message this client's \
                                     protocol cannot decode. Ignoring the message."
                                );
                                None
                            }
                        });

                        // push out rejection
                        self.incoming_world_events.push_rejection(
                            old_socket_addr,
                            RejectReason::Auth,
                            reject_message,
                        );
                    } else {
                        // push out error
                        match old_socket_addr_result {
                            Ok(_) => {
                                self.incoming_world_events
                                    .push_error(NaiaClientError::IdError(code));
                            }
                            Err(err) => {
                                self.incoming_world_events.push_error(err);
                            }
                        }
                    }

                    return;
                }
            }
        }

        // receive from socket
        loop {
            match self.io.recv_reader() {
                Ok(Some(mut reader)) => {
                    if !self.handshake_first_inbound_logged {
                        self.handshake_first_inbound_logged = true;
                        warn!("naia: Client: first inbound server datagram during handshake");
                    }
                    // The server is alive: a full silence window with no
                    // inbound traffic is the only thing that may end the
                    // attempt, so any packet re-arms the give-up timer.
                    self.handshake_timeout.reset();
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
                            warn!(
                                "naia: Client: handshake Connected to Server at {:?}",
                                server_addr
                            );
                            self.post_connect_watch = Some(PostConnectWatch::new());
                            warn!("naia: Client post-connect watch started");
                            self.incoming_world_events.push_connection(&server_addr);

                            // Stop reading here — any remaining packets in
                            // the transport (e.g. Data packets with entity
                            // replication) must be processed through
                            // maintain_connection, not the handshake loop
                            // which silently discards non-handshake packets.
                            break;
                        }
                        Some(HandshakeResult::Rejected(reason)) => {
                            info!("Client: Received HandshakeResult::Rejected({reason:?})");
                            let server_addr = self.server_address_unwrapped();
                            // The in-band handshake rejection carries no
                            // message; only the auth (401) path can (#133).
                            // In-band means post-address by construction, so
                            // this is always Some(actual_addr).
                            self.incoming_world_events.push_rejection(
                                Some(server_addr),
                                reason,
                                None,
                            );
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
                    // [b4-oom] Record one error and return: a
                    // persistently-failing socket must not trap the
                    // handshake drain loop (same shape as the server
                    // RecvState bug).
                    self.incoming_world_events
                        .push_error(NaiaClientError::Wrapped(Box::new(error)));
                    break;
                }
            }
        }
    }

    fn maintain_connection(&mut self) {
        // connection already established

        // Post-Connected liveness marker (Usher 42587 fork (a)): one-shot
        // summaries so a single run separates "stopped sending" from
        // "sent but lost". No summaries fire if the game stops polling.
        if let Some(watch) = self.post_connect_watch.as_mut() {
            let elapsed_secs = watch.connected_at.elapsed(&Instant::now()).as_secs();
            let (fire_30, fire_60) = watch.summaries_due(elapsed_secs);
            if fire_30 {
                warn!(
                    "naia: Client post-connect +30s: data_rx={} data_applied={} keepalive_sent={} last_keepalive_ok={:?}",
                    watch.data_rx, watch.data_applied, watch.keepalive_sent, watch.last_keepalive_ok
                );
            }
            if fire_60 {
                warn!(
                    "naia: Client post-connect +60s: data_rx={} data_applied={} keepalive_sent={} last_keepalive_ok={:?}",
                    watch.data_rx, watch.data_applied, watch.keepalive_sent, watch.last_keepalive_ok
                );
            }
        }

        let watch = &mut self.post_connect_watch;
        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Should have checked for this above");
        };

        Self::handle_heartbeats(connection, &mut self.io, watch);
        Self::handle_pings(connection, &mut self.io, watch);
        Self::handle_empty_acks(connection, &mut self.io, watch);

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
                            match handshake_header {
                                HandshakeHeader::Disconnect => {
                                    info!("Received disconnect from server");
                                    self.server_disconnect = true;
                                }
                                // The server said why, and may have enclosed a
                                // message explaining itself (naia-lib/naia#10).
                                HandshakeHeader::ServerDisconnect(reason) => {
                                    info!("Received disconnect from server: {reason:?}");
                                    self.server_disconnect = true;
                                    // The packet is sent several times over for
                                    // reliability; keep the first reading.
                                    if self.server_disconnect_details.is_none() {
                                        let payload =
                                            Option::<Vec<u8>>::de(&mut reader).ok().flatten();
                                        self.server_disconnect_details = Some((reason, payload));
                                    }
                                }
                                _ => {}
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

                            if let Some(w) = watch.as_mut() {
                                w.data_rx += 1;
                                if !w.first_data_warned {
                                    w.first_data_warned = true;
                                    warn!("naia: Client first post-connect data packet received");
                                }
                            }

                            if connection
                                .buffer_data_packet(&server_tick, &mut reader)
                                .is_err()
                            {
                                warn!("unable to parse data packet");
                                continue;
                            }

                            if let Some(w) = watch.as_mut() {
                                w.data_applied += 1;
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
                        // Not collapsed into a match guard: read_pong mutates the
                        // reader and the time manager, and a guard that big a side
                        // effect reads as a pure test at the match head.
                        #[allow(clippy::collapsible_match)]
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
                    // [b4-oom] Record one error and return: a
                    // persistently-failing socket must not trap the
                    // connection drain loop either (identical shape to the
                    // handshake loop above; covered by its RED test as proxy
                    // — driving maintain_connection needs a full Connection).
                    self.incoming_world_events
                        .push_error(NaiaClientError::Wrapped(Box::new(error)));
                    break;
                }
            }
        }

        if received_any {
            connection.process_received_commands();
        }
    }

    fn handle_heartbeats(
        connection: &mut Connection,
        io: &mut Io,
        watch: &mut Option<PostConnectWatch>,
    ) {
        // send heartbeats
        if connection.base.should_send_heartbeat() {
            Self::send_heartbeat_packet(connection, io, watch);
        }
    }

    fn handle_empty_acks(
        connection: &mut Connection,
        io: &mut Io,
        watch: &mut Option<PostConnectWatch>,
    ) {
        // send empty acks
        if connection.base.should_send_empty_ack() {
            Self::send_heartbeat_packet(connection, io, watch);
        }
    }

    fn send_heartbeat_packet(
        connection: &mut Connection,
        io: &mut Io,
        watch: &mut Option<PostConnectWatch>,
    ) {
        let mut writer = BitWriter::new();

        // write header
        let _header = connection
            .base
            .write_header(PacketType::Heartbeat, &mut writer);

        // send packet
        let send_ok = io.send_packet(writer.to_packet()).is_ok();
        if !send_ok {
            // Heartbeat send failure is not fatal: the server's connection
            // timeout will fire if heartbeats stop arriving persistently.
            warn!("Client Error: Cannot send heartbeat packet to Server");
        }
        if let Some(w) = watch.as_mut() {
            w.keepalive_sent += 1;
            w.last_keepalive_ok = Some(send_ok);
        }
        connection.mark_sent();
    }

    fn handle_pings(
        connection: &mut Connection,
        io: &mut Io,
        watch: &mut Option<PostConnectWatch>,
    ) {
        // send pings
        if connection.time_manager.send_ping(io) {
            if let Some(w) = watch.as_mut() {
                w.keepalive_sent += 1;
                w.last_keepalive_ok = Some(true);
            }
            connection.mark_sent();
        }
    }

    /// Resolve which disconnect reason to report for a disconnecting client.
    ///
    /// Pure mapping, extracted for tests: a server-initiated disconnect
    /// names its own reason (falling back to `ClientDisconnected` for one
    /// would tell the client it hung up on itself — naia-lib/naia#10); a
    /// locally-initiated or server-flagged teardown without details reports
    /// `ClientDisconnected`; a bare connection drop reports `TimedOut`.
    fn resolve_disconnect_reason(
        server_details: Option<(naia_shared::DisconnectReason, Option<Vec<u8>>)>,
        manual_disconnect: bool,
        server_disconnect: bool,
    ) -> (naia_shared::DisconnectReason, Option<Vec<u8>>) {
        if let Some((reason, payload)) = server_details {
            (reason, payload)
        } else {
            let reason = if manual_disconnect || server_disconnect {
                naia_shared::DisconnectReason::ClientDisconnected
            } else {
                naia_shared::DisconnectReason::TimedOut
            };
            (reason, None)
        }
    }

    fn disconnect_with_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        reason: naia_shared::DisconnectReason,
        message: Option<MessageContainer>,
    ) {
        let server_addr = self.server_address_unwrapped();

        self.incoming_world_events.clear();
        self.incoming_tick_events.clear();

        self.despawn_all_remote_entities(world);
        self.disconnect_reset_connection();

        self.incoming_world_events
            .push_disconnection(&server_addr, reason, message);
    }

    /// Decodes a message the server enclosed with a disconnect, against this
    /// client's own protocol (naia-lib/naia#10).
    fn decode_server_message(&self, bytes: &[u8]) -> Option<MessageContainer> {
        let mut reader = BitReader::new(bytes);
        if let Ok(container) = self
            .protocol
            .message_kinds
            .read(&mut reader, &FakeEntityConverter)
        {
            Some(container)
        } else {
            warn!("Server sent a disconnect message this client's protocol cannot decode. Ignoring the message.");
            None
        }
    }

    fn despawn_all_remote_entities<W: WorldMutType<E>>(&mut self, world: &mut W) {
        // this is very similar to the newtype method .. can we coalesce and reduce
        // duplication?

        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Client is already disconnected!");
        };

        let remote_entities = connection.base.send.world_manager.remote_entities();
        // Teardown synthesizes mirror-removal events: stamp the last applied
        // server tick, the tick at which these entities last existed for us.
        let teardown_tick = connection.time_manager.client_receiving_tick;
        let entity_events = SharedGlobalWorldManager::despawn_all_entities(
            world,
            &self.global_entity_map,
            &self.global_world_manager,
            remote_entities,
            teardown_tick,
        );
        self.process_entity_events(world, entity_events);
    }

    fn disconnect_reset_connection(&mut self) {
        self.server_connection = None;

        self.reset_attempt_state();

        let mut global_world_manager = GlobalWorldManager::new();
        global_world_manager.init_protocol_kind_count(self.protocol.component_kinds.kind_count());
        self.global_world_manager = global_world_manager;
    }

    /// Drops any in-flight attempt state (socket, handshake progress,
    /// disconnect flags) without touching world state or emitting events.
    ///
    /// Shared by `disconnect_reset_connection` (entities already despawned,
    /// disconnect event already queued) and `cancel_connect` (nothing was
    /// ever established, so there is nothing else to tear down).
    fn reset_attempt_state(&mut self) {
        // Tear down the previous attempt's transport BEFORE dropping it: on
        // WebRTC backends the peer outlives its Io (kept alive by JS
        // closures), so replacing Io without this leaves a stale peer
        // gathering and POSTing while the next attempt dials (Drake 42499
        // s1). Safe on an empty Io and safe to repeat.
        self.io.shutdown();
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
        // DWO-2: a processed explicit server disconnect must not poison the
        // next attempt on this client. Its reason/message were already
        // delivered with the disconnect event (or never existed); retaining
        // the flags makes the first post-handshake tick take the disconnect
        // path again — a spurious disconnect with a wrong
        // `ClientDisconnected` reason, and the client never settles
        // connected.
        self.server_disconnect = false;
        self.server_disconnect_details = None;
        // A rung give-up timer must not leak into the next attempt either.
        self.handshake_timeout.reset();
        // Same for the phase-logging attempt scope: the next dial reports
        // its own first send, first inbound datagram, and failure count.
        self.handshake_failed_sends = 0;
        self.handshake_first_send_logged = false;
        self.handshake_first_inbound_logged = false;
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
            .send
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
            .send
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
                EntityEvent::Spawn(tick, global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.incoming_world_events.push_spawn(tick, world_entity);
                    self.global_world_manager
                        .remote_spawn_entity(&global_entity);
                    let Some(connection) = self.server_connection.as_mut() else {
                        panic!("Client is disconnected!");
                    };
                    connection
                        .base
                        .send
                        .world_manager
                        .remote_spawn_entity(global_entity); // TODO: move to localworld?
                    #[cfg(feature = "e2e_debug")]
                    {
                        use crate::counters::CLIENT_SCOPE_APPLIED_ADD_E2;
                        use std::sync::atomic::Ordering;
                        CLIENT_SCOPE_APPLIED_ADD_E2.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::Despawn(tick, global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    // Resource registry maintenance: if this entity was
                    // a resource entity, clear the registry record so
                    // future has_resource::<R>() calls return false.
                    self.resource_registry.remove_by_entity(global_entity);
                    self.incoming_world_events.push_despawn(tick, world_entity);
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
                    self.global_entity_map.despawn_by_global(global_entity);
                    #[cfg(feature = "e2e_debug")]
                    {
                        use crate::counters::CLIENT_SCOPE_APPLIED_REMOVE_E1;
                        use std::sync::atomic::Ordering;
                        CLIENT_SCOPE_APPLIED_REMOVE_E1.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::InsertComponent(tick, global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    // Resource registry maintenance: if the inserted
                    // component is a Replicated Resource kind, record
                    // the (TypeId, GlobalEntity) mapping so the bevy
                    // adapter's mirror system + has_resource::<R>()
                    // lookups work O(1).
                    if self.protocol.resource_kinds.is_resource(component_kind) {
                        let type_id: std::any::TypeId = component_kind.into();
                        let _ = self.resource_registry.insert_raw(type_id, global_entity);
                    }
                    self.incoming_world_events
                        .push_insert(tick, world_entity, component_kind);

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
                                component_kind,
                            );
                            world.component_enable_delegation(
                                &self.protocol.component_kinds,
                                &self.global_entity_map,
                                &self.global_world_manager,
                                &world_entity,
                                component_kind,
                            );
                        }

                        self.global_world_manager
                            .remote_insert_component(&global_entity, &component_kind);
                    }
                }
                EntityEvent::RemoveComponent(tick, global_entity, component_box) => {
                    let component_kind = component_box.kind();
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_remove(tick, world_entity, component_box);
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
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.publish_entity(&global_entity, false);
                    self.incoming_world_events.push_publish(world_entity);
                }
                EntityEvent::Unpublish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.unpublish_entity(&global_entity, false);
                    self.incoming_world_events.push_unpublish(world_entity);
                }
                #[cfg(feature = "entity_delegation")]
                EntityEvent::EnableDelegation(global_entity) => {
                    #[cfg(feature = "e2e_debug")]
                    naia_shared::e2e_trace!(
                        "[CLIENT_RECV] EnableDelegation entity={:?}",
                        global_entity
                    );
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                        .unwrap();

                    self.entity_enable_delegation(world, &global_entity, &world_entity, false);

                    // Send EnableDelegationEntityResponse action via EntityActionEvent system
                    let Some(connection) = &mut self.server_connection else {
                        return;
                    };
                    connection
                        .base
                        .send
                        .world_manager
                        .send_enable_delegation_response(global_entity); // TODO: move to localworld?
                }
                #[cfg(feature = "entity_delegation")]
                EntityEvent::EnableDelegationResponse(_global_entity) => {
                    panic!("Client should never receive an EnableDelegationEntityResponse event");
                }
                #[cfg(feature = "entity_delegation")]
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
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.entity_disable_delegation(world, &global_entity, &world_entity, false);
                }
                #[cfg(feature = "entity_delegation")]
                EntityEvent::RequestAuthority(_global_entity) => {
                    panic!("Client should never receive an EntityRequestAuthority event");
                }
                #[cfg(feature = "entity_delegation")]
                EntityEvent::ReleaseAuthority(_global_entity) => {
                    panic!("Client should never receive an EntityReleaseAuthority event");
                }
                #[cfg(feature = "entity_delegation")]
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
                        .global_entity_to_entity(global_entity)
                        .unwrap();
                    self.entity_update_authority(&global_entity, &world_entity, new_auth_status);
                }
                // Card 27945: without the feature every delegation wire command
                // fails closed with the named error (never applied, never
                // panicked). The enum and its tags stay unconditional so the
                // wire is stable; only handling is compiled out.
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::EnableDelegation(global_entity) => {
                    self.record_delegation_disabled("EnableDelegation", &global_entity);
                    continue;
                }
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::EnableDelegationResponse(global_entity) => {
                    self.record_delegation_disabled("EnableDelegationResponse", &global_entity);
                    continue;
                }
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::DisableDelegation(global_entity) => {
                    self.record_delegation_disabled("DisableDelegation", &global_entity);
                    continue;
                }
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::RequestAuthority(global_entity) => {
                    self.record_delegation_disabled("RequestAuthority", &global_entity);
                    continue;
                }
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::ReleaseAuthority(global_entity) => {
                    self.record_delegation_disabled("ReleaseAuthority", &global_entity);
                    continue;
                }
                #[cfg(not(feature = "entity_delegation"))]
                EntityEvent::SetAuthority(global_entity, _) => {
                    self.record_delegation_disabled("SetAuthority", &global_entity);
                    continue;
                }
                EntityEvent::MigrateResponse(global_entity, new_remote_entity) => {
                    // Validate we have a valid world entity
                    let world_entity = if let Ok(entity) = self
                        .global_entity_map
                        .global_entity_to_entity(global_entity)
                    {
                        entity
                    } else {
                        warn!(
                            "Received MigrateResponse for unknown global entity: {global_entity:?}"
                        );
                        return;
                    };

                    // Scope the connection borrow to complete migration steps
                    {
                        let Some(connection) = &mut self.server_connection else {
                            warn!("Received MigrateResponse without active server connection");
                            return;
                        };

                        let old_host_entity = if let Ok(entity) = connection
                            .base
                            .send
                            .world_manager
                            .entity_converter()
                            .global_entity_to_host_entity(global_entity)
                        {
                            entity
                        } else {
                            warn!(
                                "Entity {global_entity:?} does not exist as HostEntity before migration"
                            );
                            return;
                        };

                        // Extract and buffer outgoing commands to preserve pending operations
                        let buffered_commands = connection
                            .base
                            .send
                            .world_manager
                            .extract_host_entity_commands(global_entity);

                        // Extract component state to preserve during migration
                        let component_kinds = connection
                            .base
                            .send
                            .world_manager
                            .extract_host_component_kinds(global_entity);

                        // Remove old HostEntityChannel
                        connection
                            .base
                            .send
                            .world_manager
                            .remove_host_entity(global_entity);

                        // Create new RemoteEntityChannel with preserved component state
                        connection.base.send.world_manager.insert_remote_entity(
                            global_entity,
                            new_remote_entity,
                            component_kinds,
                        );

                        // Install entity redirect for old references
                        let old_entity = OwnedLocalEntity::Host {
                            id: old_host_entity.value(),
                            is_static: false,
                        };
                        let new_entity = new_remote_entity.copy_to_owned();
                        connection
                            .base
                            .send
                            .world_manager
                            .install_entity_redirect(old_entity, new_entity);

                        // Update pending command packet references
                        connection
                            .base
                            .send
                            .world_manager
                            .update_sent_command_entity_refs(global_entity, old_entity, new_entity);

                        // Replay buffered commands
                        for command in buffered_commands {
                            if command.is_valid_for_remote_entity() {
                                connection
                                    .base
                                    .send
                                    .world_manager
                                    .replay_entity_command(global_entity, command);
                            }
                        }

                        // Update RemoteEntityChannel's internal AuthChannel status
                        // After migration, grant authority back to the creating client
                        connection
                            .base
                            .send
                            .world_manager
                            .remote_receive_set_auth(global_entity, EntityAuthStatus::Granted);
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

                    // Register the entity + its components for outgoing update
                    // tracking, exactly as the SetAuthority(Granted) event path
                    // does. Without this the migrated entity's `authed_entities`
                    // entry is never created, so `is_component_updatable` stays
                    // false and every c2s component update the (granted) owner
                    // makes is silently dropped from `take_update_events` —
                    // the entity LOOKS granted everywhere (global status,
                    // channel status, AuthGrant event) but can never sync.
                    if let Some(connection) = &mut self.server_connection {
                        connection
                            .base
                            .send
                            .world_manager
                            .register_authed_entity(&self.global_world_manager, global_entity);
                    }

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
                        .global_entity_to_entity(global_entity)
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
        global_entity: GlobalEntity,
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
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self == &ConnectionStatus::Disconnected
    }

    /// Returns `true` if the handshake is in progress.
    #[must_use]
    pub fn is_connecting(&self) -> bool {
        self == &ConnectionStatus::Connecting
    }

    /// Returns `true` if the connection is active.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self == &ConnectionStatus::Connected
    }

    /// Returns `true` if the client is tearing down an active connection.
    #[must_use]
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

                connection.base.send.world_manager.local_entities()
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
                let converter = connection.base.send.world_manager.entity_converter();

                let owned_local_entity: OwnedLocalEntity = (*local_entity).into();
                let global_entity = converter.owned_entity_to_global_entity(owned_local_entity).ok()?;
                let world_entity = self
                    .global_entity_map
                    .global_entity_to_entity(global_entity)
                    .ok()?;

                Some(world_entity)
            }

            pub(crate) fn world_to_local_entity(
                &self,
                world_entity: &E,
            ) -> Option<LocalEntity> {
                let global_entity = self.global_entity_map.entity_to_global_entity(world_entity).ok()?;

                let connection = self.server_connection.as_ref()?;
                let converter = connection.base.send.world_manager.entity_converter();
                let owned_entity = converter.global_entity_to_owned_entity(global_entity).ok()?;

                Some(LocalEntity::from(owned_entity))
            }
        }
    }
}

// ---- [b4-oom] drain-termination tests: a persistently-failing transport
// (disconnected socket at teardown) must not trap the handshake drain loop.
// Pre-fix the Err arm pushed one Boxed error per spin with no break (same
// shape as the server RecvState bug); the thread below fails by timeout
// instead of hanging the suite forever.
#[cfg(test)]
mod drain_termination_tests {
    use std::net::SocketAddr;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use naia_shared::{IdentityToken, Protocol};

    use crate::transport::{
        IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError,
        SendError, ServerAddr,
    };
    use crate::world_events::ErrorEvent;

    use super::*;

    fn dummy_server() -> ServerAddr {
        ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
    }

    #[derive(Clone)]
    struct OkIdReceiver;

    impl IdentityReceiver for OkIdReceiver {
        fn receive(&mut self) -> IdentityReceiverResult {
            IdentityReceiverResult::Success(IdentityToken::generate())
        }
    }

    #[derive(Clone)]
    struct OkPacketSender;

    impl PacketSender for OkPacketSender {
        fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
            Ok(())
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }

        fn shutdown(&mut self) {}
    }

    #[derive(Clone)]
    struct FailPacketReceiver;

    impl PacketReceiver for FailPacketReceiver {
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
            Err(RecvError)
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }
    }

    #[test]
    fn maintain_handshake_terminates_on_persistent_transport_error() {
        let mut client = Client::<u64>::new(ClientConfig::default(), Protocol::builder().build());
        client.io.load(
            Box::new(OkIdReceiver),
            Box::new(OkPacketSender),
            Box::new(FailPacketReceiver),
        );

        let client = Arc::new(Mutex::new(client));
        let worker = client.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            worker.lock().unwrap().maintain_handshake();
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(10))
            .expect("maintain_handshake must return on a persistently-failing transport");
        // The loop hit its Err arm (not skipped): exactly one error.
        let errors = client
            .lock()
            .unwrap()
            .incoming_world_events
            .read::<ErrorEvent>()
            .count();
        assert_eq!(
            errors, 1,
            "one error per maintain_handshake call, not one per spin"
        );
    }
}

// ---- Handshake send accounting (Usher 42357): failed handshake sends are
// counted per attempt so the first success can report how lossy the path
// was; the success resets the counter for the attempt.
#[cfg(test)]
mod handshake_send_accounting_tests {
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use naia_shared::{
        ComponentKind, IdentityToken, Protocol, ReplicaDynRefWrapper, ReplicaRefWrapper,
        ReplicatedComponent, WorldRefType,
    };

    use crate::transport::{
        IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError,
        SendError, ServerAddr,
    };

    use super::*;

    /// Empty world: the handshake send branch never touches it, but the
    /// `send_all_packets` bound still needs a value.
    struct StubWorld;

    impl WorldRefType<u64> for StubWorld {
        fn has_entity(&self, _world_entity: &u64) -> bool {
            false
        }
        fn entities(&self) -> Vec<u64> {
            Vec::new()
        }
        fn has_component<R: ReplicatedComponent>(&self, _world_entity: &u64) -> bool {
            false
        }
        fn has_component_of_kind(
            &self,
            _world_entity: &u64,
            _component_kind: &ComponentKind,
        ) -> bool {
            false
        }
        fn component<'a, R: ReplicatedComponent>(
            &'a self,
            _entity: &u64,
        ) -> Option<ReplicaRefWrapper<'a, R>> {
            None
        }
        fn component_of_kind<'a>(
            &'a self,
            _entity: &u64,
            _component_kind: &ComponentKind,
        ) -> Option<ReplicaDynRefWrapper<'a>> {
            None
        }
    }

    #[derive(Clone)]
    struct OkIdReceiver;

    impl IdentityReceiver for OkIdReceiver {
        fn receive(&mut self) -> IdentityReceiverResult {
            IdentityReceiverResult::Success(IdentityToken::generate())
        }
    }

    /// Fails the first two sends, then succeeds: the attempt must report
    /// exactly the failures that preceded its first success.
    struct FailTwiceSender {
        attempts: AtomicUsize,
    }

    impl PacketSender for FailTwiceSender {
        fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
            if self.attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(SendError)
            } else {
                Ok(())
            }
        }
        fn server_addr(&self) -> ServerAddr {
            ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
        }
        fn shutdown(&mut self) {}
    }

    #[derive(Clone)]
    struct QuietReceiver;

    impl PacketReceiver for QuietReceiver {
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
            Ok(None)
        }
        fn server_addr(&self) -> ServerAddr {
            ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
        }
    }

    #[test]
    fn handshake_send_failures_are_counted_until_the_first_success() {
        // Zero send interval so consecutive pump calls each attempt a send
        // without waiting out the production 250 ms spacing.
        let config = ClientConfig {
            send_handshake_interval: Duration::ZERO,
            ..Default::default()
        };
        let mut client = Client::<u64>::new(config, Protocol::builder().build());
        client.io.load(
            Box::new(OkIdReceiver),
            Box::new(FailTwiceSender {
                attempts: AtomicUsize::new(0),
            }),
            Box::new(QuietReceiver),
        );
        client
            .handshake_manager
            .set_identity_token(IdentityToken::generate());

        client.send_all_packets(StubWorld);
        client.send_all_packets(StubWorld);
        assert_eq!(
            client.handshake_failed_sends, 2,
            "the two failed sends must be counted before the first success",
        );
        assert!(
            !client.handshake_first_send_logged,
            "no success yet, so the first-success line must not have fired",
        );

        client.send_all_packets(StubWorld);
        assert!(
            client.handshake_first_send_logged,
            "the first successful send must fire the once-per-attempt line",
        );
        assert_eq!(
            client.handshake_failed_sends, 0,
            "the first success resets the attempt counter",
        );
    }
}

// ---- Post-Connected liveness watch (Usher 42587 fork a, oracle pins) ----
// The +30s/+60s summaries and the first-data line are operator signals:
// the tests below fail if the markers or their schedule are removed.
#[cfg(test)]
mod post_connect_watch_tests {
    use super::*;

    #[test]
    fn watch_starts_unfired_with_zeroed_counters() {
        let watch = PostConnectWatch::new();
        assert_eq!(watch.data_rx, 0);
        assert_eq!(watch.data_applied, 0);
        assert_eq!(watch.keepalive_sent, 0);
        assert_eq!(watch.last_keepalive_ok, None);
        assert!(!watch.first_data_warned);
        assert!(!watch.summary_30_warned);
        assert!(!watch.summary_60_warned);
    }

    #[test]
    fn summaries_fire_once_at_30s_and_60s() {
        let mut watch = PostConnectWatch::new();
        assert_eq!(watch.summaries_due(29), (false, false));
        assert_eq!(watch.summaries_due(30), (true, false));
        assert_eq!(
            watch.summaries_due(31),
            (false, false),
            "the +30s summary must not repeat",
        );
        assert_eq!(watch.summaries_due(59), (false, false));
        assert_eq!(watch.summaries_due(60), (false, true));
        assert_eq!(
            watch.summaries_due(61),
            (false, false),
            "the +60s summary must not repeat",
        );
    }
}

// ---- N1-b remainder (27202/4): the client's send path must refuse with a
// typed variant when no connection exists, like every other send_request arm.
#[cfg(test)]
mod typed_refusal_tests {
    use naia_shared::{
        Channel, ChannelDirection, ChannelMode, Message, Protocol, ReliableSettings, Request,
        Response,
    };

    use super::*;
    use crate::NaiaClientError;

    #[derive(Channel)]
    struct TestRequestChannel;

    #[derive(Message)]
    struct TestRequest {
        query: u32,
    }

    #[derive(Message)]
    struct TestResponse {
        result: u32,
    }

    impl Request for TestRequest {
        type Response = TestResponse;
    }

    impl Response for TestResponse {}

    #[derive(Channel)]
    struct TestCappedChannel;

    #[derive(Message)]
    struct TestCappedMessage {
        text: u32,
    }

    #[test]
    fn send_request_before_connect_returns_not_connected() {
        let mut proto = Protocol::builder();
        proto
            .add_channel::<TestRequestChannel>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedReliable(ReliableSettings::default()),
            )
            .add_message::<TestRequest>()
            .add_message::<TestResponse>();
        // Unlocked: Client::new locks the protocol itself.
        let protocol = proto.build();

        let mut client = Client::<u64>::new(ClientConfig::default(), protocol);
        let request = TestRequest { query: 7 };
        assert!(matches!(
            client.send_request::<TestRequestChannel, _>(&request),
            Err(NaiaClientError::NotConnected)
        ));
    }

    /// Census 29818 Hit A: the pre-connect waitlist is the queue, so the
    /// channel's `max_queue_depth` must apply to it exactly as it does on
    /// the live path. Otherwise disconnected submits bypass the documented
    /// `MessageQueueFull` backpressure without bound (and the MVP-path chat
    /// must track transport failure in parallel because every submit
    /// answers Ok).
    #[test]
    fn send_message_before_connect_refuses_past_queue_cap() {
        let mut proto = Protocol::builder();
        proto
            .add_channel::<TestCappedChannel>(
                ChannelDirection::ClientToServer,
                ChannelMode::UnorderedReliable(ReliableSettings {
                    max_queue_depth: Some(2),
                    ..ReliableSettings::default()
                }),
            )
            .add_message::<TestCappedMessage>();
        let protocol = proto.build();

        let mut client = Client::<u64>::new(ClientConfig::default(), protocol);
        let message = TestCappedMessage { text: 1 };
        assert!(client
            .send_message::<TestCappedChannel, _>(&message)
            .is_ok());
        assert!(client
            .send_message::<TestCappedChannel, _>(&message)
            .is_ok());
        assert!(matches!(
            client.send_message::<TestCappedChannel, _>(&message),
            Err(NaiaClientError::MessageQueueFull)
        ));
    }
}

// ---- Item 2 (Usher 38239): client lifecycle disconnect transitions.
//
// `disconnect` / `cancel_connect` / `connection_status` form the client side
// of the reconnect/disconnect contract: graceful teardown emits packets and
// moves Connected -> Disconnecting, abandoning a stuck handshake returns to
// Disconnected without touching a live connection, and teardown on a client
// that never connected is a panic, not a silent no-op.
#[cfg(test)]
mod client_disconnect_tests {
    use std::net::SocketAddr;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::time::Duration;

    #[cfg(feature = "test_time")]
    use naia_shared::TestClock;
    use naia_shared::{DisconnectReason, GameInstant, IdentityToken, Instant, Protocol};

    #[cfg(feature = "test_time")]
    use crate::world_events::{DisconnectEvent, WorldEvent};

    use crate::connection::time_manager::TimeManager;
    use crate::transport::{
        IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError,
        SendError, ServerAddr,
    };

    use super::*;

    fn dummy_server() -> ServerAddr {
        ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
    }

    #[derive(Clone)]
    struct OkIdReceiver;

    impl IdentityReceiver for OkIdReceiver {
        fn receive(&mut self) -> IdentityReceiverResult {
            IdentityReceiverResult::Success(IdentityToken::generate())
        }
    }

    #[derive(Clone)]
    struct CountingSender {
        sent: Arc<AtomicUsize>,
    }

    impl PacketSender for CountingSender {
        fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
            self.sent.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }

        fn shutdown(&mut self) {}
    }

    #[derive(Clone)]
    struct EmptyReceiver;

    impl PacketReceiver for EmptyReceiver {
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
            Ok(None)
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }
    }

    fn idle_client() -> Client<u64> {
        Client::<u64>::new(ClientConfig::default(), Protocol::builder().build())
    }

    fn loading_client(sender: CountingSender) -> Client<u64> {
        loading_client_with_receiver(Box::new(sender), Box::new(EmptyReceiver))
    }

    fn loading_client_with_receiver(
        sender: Box<dyn PacketSender>,
        receiver: Box<dyn PacketReceiver>,
    ) -> Client<u64> {
        let mut client = idle_client();
        client.io.load(Box::new(OkIdReceiver), sender, receiver);
        client
    }

    /// A client with an established connection, without a live server: the
    /// connection is built exactly the way `maintain_handshake` builds it on
    /// `HandshakeResult::Connected`, with pristine (zero-sample) timing.
    fn connected_client() -> (Client<u64>, Arc<AtomicUsize>) {
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client(CountingSender { sent: sent.clone() });
        client
            .handshake_manager
            .set_identity_token(IdentityToken::generate());
        let time_manager = TimeManager::from_parts(
            Duration::from_millis(100),
            BaseTimeManager::new(),
            0,
            GameInstant::new(&Instant::now()),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        client.server_connection = Some(Connection::new(
            &client.client_config.connection,
            &client.protocol.channel_kinds,
            time_manager,
            &client.global_world_manager,
            client.client_config.jitter_buffer,
            &client.protocol.component_kinds,
        ));
        (client, sent)
    }

    #[test]
    #[should_panic(expected = "not connected yet")]
    fn disconnect_panics_when_not_connected() {
        let mut client = idle_client();
        client.disconnect();
    }

    #[test]
    fn cancel_connect_on_idle_stays_disconnected() {
        let mut client = idle_client();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        client.cancel_connect();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn cancel_connect_abandons_pending_handshake() {
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client(CountingSender { sent: sent.clone() });
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        client.cancel_connect();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        // Abandoning sends nothing: teardown of an unestablished attempt is
        // silent (no graceful-disconnect packets for a connection that never
        // existed).
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancel_connect_leaves_live_connection_alone() {
        let (mut client, _sent) = connected_client();
        assert_eq!(client.connection_status(), ConnectionStatus::Connected);
        client.cancel_connect();
        assert_eq!(client.connection_status(), ConnectionStatus::Connected);
        assert!(client.server_connection.is_some());
    }

    #[test]
    fn disconnect_sends_ten_packets_then_disconnecting() {
        let (mut client, sent) = connected_client();
        client.disconnect();
        assert_eq!(
            sent.load(Ordering::SeqCst),
            10,
            "graceful disconnect must emit exactly 10 packets",
        );
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnecting);
    }

    #[test]
    fn server_named_reason_wins_over_local_flags() {
        // naia-lib/naia#10: a server-initiated disconnect names its own
        // reason; falling back to ClientDisconnected would tell the client
        // it hung up on itself — even when the manual flag is also set.
        let payload = Some(vec![7u8; 4]);
        let (reason, out_payload) = Client::<u64>::resolve_disconnect_reason(
            Some((DisconnectReason::Kicked, payload.clone())),
            true,
            true,
        );
        assert_eq!(reason, DisconnectReason::Kicked);
        assert_eq!(out_payload, payload);
    }

    #[test]
    fn manual_disconnect_maps_to_client_disconnected() {
        let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, true, false);
        assert_eq!(reason, DisconnectReason::ClientDisconnected);
        assert!(payload.is_none());
    }

    #[test]
    fn server_disconnect_flag_maps_to_client_disconnected() {
        let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, false, true);
        assert_eq!(reason, DisconnectReason::ClientDisconnected);
        assert!(payload.is_none());
    }

    #[test]
    fn bare_connection_drop_maps_to_timed_out() {
        let (reason, payload) = Client::<u64>::resolve_disconnect_reason(None, false, false);
        assert_eq!(reason, DisconnectReason::TimedOut);
        assert!(payload.is_none());
    }

    /// A scripted inbound transport: yields each packet once, then silence.
    /// Lets established-state tests drive `maintain_socket` with exact wire
    /// bytes, the way the server's `write_disconnect` emits them.
    #[derive(Clone)]
    struct ScriptedReceiver {
        packets: Vec<Vec<u8>>,
        next: usize,
    }

    impl PacketReceiver for ScriptedReceiver {
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
            let out = self.packets.get(self.next).map(Vec::as_slice);
            if out.is_some() {
                self.next += 1;
            }
            Ok(out)
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }
    }

    /// Byte-identical to the server's `write_disconnect`: standard header +
    /// `ServerDisconnect` header + optional message payload.
    fn server_disconnect_packet(reason: DisconnectReason, payload: Option<&[u8]>) -> Vec<u8> {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Handshake, 0, 0, 0).ser(&mut writer);
        HandshakeHeader::ServerDisconnect(reason).ser(&mut writer);
        payload.map(|bytes| bytes.to_vec()).ser(&mut writer);
        writer.to_packet().slice().to_vec()
    }

    fn connected_client_with_receiver(
        sender: Box<dyn PacketSender>,
        receiver: Box<dyn PacketReceiver>,
    ) -> Client<u64> {
        let mut client = loading_client_with_receiver(sender, receiver);
        client
            .handshake_manager
            .set_identity_token(IdentityToken::generate());
        let time_manager = TimeManager::from_parts(
            Duration::from_millis(100),
            BaseTimeManager::new(),
            0,
            GameInstant::new(&Instant::now()),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        client.server_connection = Some(Connection::new(
            &client.client_config.connection,
            &client.protocol.channel_kinds,
            time_manager,
            &client.global_world_manager,
            client.client_config.jitter_buffer,
            &client.protocol.component_kinds,
        ));
        client
    }

    #[test]
    fn repeated_server_disconnect_keeps_first_reason_and_payload() {
        // The server sends the disconnect several times for reliability; the
        // client must report the FIRST reading, not the last — otherwise a
        // terminal Kicked could be overwritten by a later packet and the
        // consumer would auto-reconnect into an eviction loop.
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = connected_client_with_receiver(
            Box::new(CountingSender { sent }),
            Box::new(ScriptedReceiver {
                packets: vec![
                    server_disconnect_packet(DisconnectReason::Kicked, Some(&[1, 2, 3])),
                    server_disconnect_packet(DisconnectReason::TimedOut, Some(&[9])),
                ],
                next: 0,
            }),
        );
        client.maintain_socket();
        assert!(client.server_disconnect);
        assert_eq!(
            client.server_disconnect_details,
            Some((DisconnectReason::Kicked, Some(vec![1u8, 2, 3]))),
            "a repeated server disconnect must not clobber the first reason",
        );
    }

    /// A bare keep-alive, the way the server's heartbeat task emits it.
    #[cfg(feature = "test_time")]
    fn heartbeat_packet() -> Vec<u8> {
        let mut writer = BitWriter::new();
        StandardHeader::new(PacketType::Heartbeat, 0, 0, 0).ser(&mut writer);
        writer.to_packet().slice().to_vec()
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn silence_past_timeout_marks_connection_for_drop() {
        // 30 s default timeout: a peer gone silent that long must read as
        // disconnecting, resolving to TimedOut downstream (safe to
        // auto-reconnect). Each test runs on its own thread so the
        // thread-local TestClock is clean.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = connected_client_with_receiver(
                Box::new(CountingSender { sent }),
                Box::new(ScriptedReceiver {
                    packets: vec![],
                    next: 0,
                }),
            );
            assert!(!client.is_disconnecting());
            TestClock::advance(30_001);
            client.maintain_socket();
            assert!(
                client.is_disconnecting(),
                "30 s of silence must mark the connection for drop",
            );
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn heard_packet_rearms_drop_timer() {
        // Any heard packet — even a bare heartbeat — resets the timeout:
        // 29 s + heartbeat + 29 s must NOT read as disconnecting, but a
        // further 2 s of silence must.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = connected_client_with_receiver(
                Box::new(CountingSender { sent }),
                Box::new(ScriptedReceiver {
                    packets: vec![heartbeat_packet()],
                    next: 0,
                }),
            );
            TestClock::advance(29_000);
            client.maintain_socket();
            assert!(!client.is_disconnecting());
            TestClock::advance(29_000);
            client.maintain_socket();
            assert!(
                !client.is_disconnecting(),
                "a heartbeat 29 s ago must still hold the connection",
            );
            TestClock::advance(2_000);
            client.maintain_socket();
            assert!(
                client.is_disconnecting(),
                "31 s after the last heard packet must mark drop",
            );
        })
        .join()
        .unwrap();
    }

    /// A sender that keeps every payload, so keep-alive tests can inspect
    /// what the client actually emitted while silent.
    #[cfg(feature = "test_time")]
    #[derive(Clone)]
    struct CapturingSender {
        sent: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    }

    #[cfg(feature = "test_time")]
    impl PacketSender for CapturingSender {
        fn send(&self, payload: &[u8]) -> Result<(), SendError> {
            self.sent.lock().unwrap().push(payload.to_vec());
            Ok(())
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }

        fn shutdown(&mut self) {}
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn silent_client_emits_keep_alive_past_interval() {
        // 4 s default heartbeat interval: the silent side must speak first,
        // or the peer's 30 s drop timer fires on a merely-idle connection
        // and a healthy link flaps through TimedOut reconnects. The client
        // keeps alive with Ping (latency probe) and Heartbeat packets —
        // never Data.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
            let mut client = connected_client_with_receiver(
                Box::new(CapturingSender { sent: sent.clone() }),
                Box::new(ScriptedReceiver {
                    packets: vec![],
                    next: 0,
                }),
            );
            // Idle past the heartbeat interval: the client must speak.
            TestClock::advance(4_001);
            client.maintain_socket();
            let sent = sent.lock().unwrap();
            assert!(
                !sent.is_empty(),
                "a silent client must emit a keep-alive past the heartbeat interval",
            );
            for payload in sent.iter() {
                let mut reader = BitReader::new(payload);
                let header = StandardHeader::de(&mut reader).expect("keep-alive must parse");
                assert!(
                    matches!(header.packet_type, PacketType::Ping | PacketType::Heartbeat),
                    "idle emissions must be keep-alives, not data: {:?}",
                    header.packet_type,
                );
            }
        })
        .join()
        .unwrap();
    }

    /// Drain the public disconnection events, so give-up tests can assert
    /// exact event counts and reasons.
    #[cfg(feature = "test_time")]
    fn take_disconnects(
        client: &mut Client<u64>,
    ) -> Vec<(SocketAddr, DisconnectReason, Option<MessageContainer>)> {
        DisconnectEvent::iter(&mut client.take_world_events()).collect()
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn silent_handshake_gives_up_with_auth_timeout_at_deadline() {
        // A server that never answers must surface exactly one AuthTimeout
        // at the 30 s link-silence deadline — never before, never twice —
        // so the consumer's offline path learns the attempt failed instead
        // of watching "connecting" forever.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = loading_client(CountingSender { sent });
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

            // Just inside the deadline: still trying, no event.
            TestClock::advance(29_999);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
            assert!(take_disconnects(&mut client).is_empty());

            // Past it: exactly one AuthTimeout carrying the dial target.
            TestClock::advance(2);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
            let events = take_disconnects(&mut client);
            assert_eq!(events.len(), 1, "give-up must emit exactly one event");
            assert_eq!(events[0].0, "127.0.0.1:9999".parse().unwrap());
            assert_eq!(events[0].1, DisconnectReason::AuthTimeout);
            assert!(events[0].2.is_none());

            // A further tick emits nothing more: the reset disarmed the timer.
            client.maintain_socket();
            assert!(take_disconnects(&mut client).is_empty());
        })
        .join()
        .unwrap();
    }

    /// A receiver that delivers exactly one undecodable packet, then
    /// silence: the peer is alive (bytes arrive) but says nothing the
    /// handshake can use. Proves heard traffic re-arms the give-up timer.
    #[cfg(feature = "test_time")]
    #[derive(Clone)]
    struct OnceReceiver {
        packet: Vec<u8>,
        live: bool,
    }

    #[cfg(feature = "test_time")]
    impl PacketReceiver for OnceReceiver {
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
            if self.live {
                self.live = false;
                Ok(Some(self.packet.as_slice()))
            } else {
                Ok(None)
            }
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn heard_handshake_traffic_rearms_give_up_timer() {
        // The give-up deadline measures link SILENCE, not time since dial:
        // a peer that keeps sending — even packets the handshake cannot
        // use — keeps the attempt alive. Without the re-arm, a
        // healthy-but-slow handshake under a short silence window reports
        // AuthTimeout mid-connect and kills the attempt (heartbeat-timeout
        // e2e regression: first-bad e567e1fe).
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = loading_client_with_receiver(
                Box::new(CountingSender { sent }),
                Box::new(OnceReceiver {
                    packet: vec![0xFF; 8],
                    live: true,
                }),
            );
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

            // Near the deadline a packet arrives: still trying, no event.
            TestClock::advance(29_999);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
            assert!(take_disconnects(&mut client).is_empty());

            // Past the ORIGINAL deadline the attempt survives: the heard
            // packet re-armed the timer.
            TestClock::advance(2);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
            assert!(take_disconnects(&mut client).is_empty());

            // A full silent window after the last heard packet still gives
            // up: exactly one AuthTimeout, attempt over.
            TestClock::advance(30_000);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
            let events = take_disconnects(&mut client);
            assert_eq!(events.len(), 1, "silence must still give up exactly once");
            assert_eq!(events[0].1, DisconnectReason::AuthTimeout);
        })
        .join()
        .unwrap();
    }

    /// A sender whose transport never learned the peer address: the dial
    /// target never resolved to a socket address.
    #[cfg(feature = "test_time")]
    #[derive(Clone)]
    struct FindingSender;

    #[cfg(feature = "test_time")]
    impl PacketSender for FindingSender {
        fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
            Ok(())
        }

        fn server_addr(&self) -> ServerAddr {
            ServerAddr::Finding
        }

        fn shutdown(&mut self) {}
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn give_up_with_unknown_peer_emits_no_event_but_resets() {
        // Deadline with the peer address never learned: there is nothing
        // honest to attribute an AuthTimeout to, so no event is
        // fabricated — but the attempt still tears down to Disconnected
        // so the consumer's retry loop re-engages instead of watching
        // "connecting" forever.
        std::thread::spawn(|| {
            TestClock::init(0);
            let mut client =
                loading_client_with_receiver(Box::new(FindingSender), Box::new(EmptyReceiver));
            assert_eq!(client.connection_status(), ConnectionStatus::Connecting);

            TestClock::advance(30_001);
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
            assert!(
                take_disconnects(&mut client).is_empty(),
                "no address means no event, never a manufactured one"
            );
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn established_connection_ignores_handshake_deadline() {
        // The give-up timer is a handshake-only instrument: a live
        // connection kept awake by heartbeats must never consult it, no
        // matter how long the attempt clock has been running.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = connected_client_with_receiver(
                Box::new(CountingSender { sent }),
                Box::new(ScriptedReceiver {
                    packets: vec![
                        heartbeat_packet(),
                        heartbeat_packet(),
                        heartbeat_packet(),
                        heartbeat_packet(),
                    ],
                    next: 0,
                }),
            );
            // 60 s total — twice the deadline — with a heartbeat every 15 s.
            for _ in 0..4 {
                TestClock::advance(15_000);
                client.maintain_socket();
            }
            assert!(client.server_connection.is_some());
            assert!(
                take_disconnects(&mut client).is_empty(),
                "an established connection must never report AuthTimeout",
            );
        })
        .join()
        .unwrap();
    }

    #[cfg(feature = "test_time")]
    #[test]
    fn cancel_before_deadline_gives_no_auth_timeout() {
        // Abandoning the attempt wins over the deadline: no event for an
        // attempt the consumer already withdrew.
        std::thread::spawn(|| {
            TestClock::init(0);
            let sent = Arc::new(AtomicUsize::new(0));
            let mut client = loading_client(CountingSender { sent });
            TestClock::advance(29_999);
            client.maintain_socket();
            client.cancel_connect();
            TestClock::advance(60_000);
            client.maintain_socket();
            client.maintain_socket();
            assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
            assert!(take_disconnects(&mut client).is_empty());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn cancel_connect_drops_poisoned_server_disconnect_flags() {
        // DWO-2: a stale explicit-disconnect flag (e.g. left by a previous
        // incarnation of this client object) must not survive into the next
        // attempt — the first post-handshake tick would otherwise take the
        // disconnect path again with a wrong ClientDisconnected reason.
        let sent = Arc::new(AtomicUsize::new(0));
        let mut client = loading_client(CountingSender { sent: sent.clone() });
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        client.server_disconnect = true;
        client.server_disconnect_details = Some((DisconnectReason::Kicked, Some(vec![1u8])));
        client.cancel_connect();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        assert!(
            !client.server_disconnect && client.server_disconnect_details.is_none(),
            "cancel must drop stale server-disconnect state with the attempt",
        );
    }

    /// One dial attempt's observable transport (Drake 42499 s1). `open` is
    /// the peer: shutdown must drive it dark. `order` records dials and
    /// shutdowns across attempts on one shared wire timeline.
    #[derive(Clone)]
    struct AttemptWire {
        order: Arc<Mutex<Vec<&'static str>>>,
    }

    #[derive(Clone)]
    struct AttemptSender {
        wire: AttemptWire,
        open: Arc<AtomicBool>,
        shutdown_tag: &'static str,
    }

    impl AttemptWire {
        fn record(&self, event: &'static str) {
            self.order.lock().unwrap().push(event);
        }
    }

    impl PacketSender for AttemptSender {
        fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
            if self.open.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(SendError)
            }
        }

        fn server_addr(&self) -> ServerAddr {
            dummy_server()
        }

        fn shutdown(&mut self) {
            // The effect, not a hook record: the peer goes dark, so a send
            // after shutdown fails. First call wins; repeats change nothing.
            if self.open.swap(false, Ordering::SeqCst) {
                self.wire.record(self.shutdown_tag);
            }
        }
    }

    /// A dial that mints a fresh attempt on one shared wire timeline.
    struct DialSocket {
        wire: AttemptWire,
        dial_tag: &'static str,
        shutdown_tag: &'static str,
        attempt_open: Arc<AtomicBool>,
    }

    impl From<DialSocket> for Box<dyn Socket> {
        fn from(val: DialSocket) -> Self {
            Box::new(val)
        }
    }

    impl Socket for DialSocket {
        fn connect(
            self: Box<Self>,
            _protocol_id: naia_shared::ProtocolId,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        ) {
            self.wire.record(self.dial_tag);
            (
                Box::new(OkIdReceiver),
                Box::new(AttemptSender {
                    wire: AttemptWire {
                        order: self.wire.order.clone(),
                    },
                    open: self.attempt_open.clone(),
                    shutdown_tag: self.shutdown_tag,
                }),
                Box::new(EmptyReceiver),
            )
        }

        fn connect_with_auth(
            self: Box<Self>,
            protocol_id: naia_shared::ProtocolId,
            _auth_bytes: Vec<u8>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        ) {
            self.connect(protocol_id)
        }

        fn connect_with_auth_headers(
            self: Box<Self>,
            protocol_id: naia_shared::ProtocolId,
            _auth_headers: Vec<(String, String)>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        ) {
            self.connect(protocol_id)
        }

        fn connect_with_auth_and_headers(
            self: Box<Self>,
            protocol_id: naia_shared::ProtocolId,
            _auth_bytes: Vec<u8>,
            _auth_headers: Vec<(String, String)>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        ) {
            self.connect(protocol_id)
        }
    }

    #[test]
    fn retry_shuts_down_previous_attempt_before_redial() {
        // s1 (Drake 42499): abandoning an attempt and dialing again must tear
        // down the previous attempt's transport first. Otherwise the stale
        // peer stays live and a second connection shares the wire mid-run.
        let order = Arc::new(Mutex::new(Vec::new()));
        let wire = AttemptWire {
            order: order.clone(),
        };
        let a1_open = Arc::new(AtomicBool::new(true));
        let mut client = idle_client();
        client.connect(DialSocket {
            wire: AttemptWire {
                order: order.clone(),
            },
            dial_tag: "dial-a1",
            shutdown_tag: "shutdown-a1",
            attempt_open: a1_open.clone(),
        });
        assert_eq!(client.connection_status(), ConnectionStatus::Connecting);
        client.cancel_connect();
        // Idempotent across repeated resets: a second cancel on the now-idle
        // client meets an empty Io and must not re-fire the teardown.
        client.cancel_connect();
        assert_eq!(client.connection_status(), ConnectionStatus::Disconnected);
        client.connect(DialSocket {
            wire,
            dial_tag: "dial-a2",
            shutdown_tag: "shutdown-a2",
            attempt_open: Arc::new(AtomicBool::new(true)),
        });
        // EFFECT: the first attempt's transport is dark — a send on it now
        // fails — and it went dark before the second attempt dialed.
        assert!(
            !a1_open.load(Ordering::SeqCst),
            "abandoned attempt's transport must be shut down on retry",
        );
        assert_eq!(
            *order.lock().unwrap(),
            vec!["dial-a1", "shutdown-a1", "dial-a2"],
            "shutdown must fire exactly once, before the redial",
        );
    }
}
