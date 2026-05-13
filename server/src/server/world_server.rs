use std::{
    any::Any,
    collections::{hash_set::Iter, HashMap, HashSet, VecDeque},
    hash::Hash,
    net::SocketAddr,
    panic,
    time::Duration,
};

use log::{info, warn};

use naia_shared::{
    handshake::HandshakeHeader, AuthorityError, BitReader, BitWriter, Channel, ChannelKind,
    ConnectionStats, DisconnectReason,
    ChannelKinds, ComponentKind, ComponentKinds, EntityAndGlobalEntityConverter, EntityAuthStatus,
    EntityDoesNotExistError, EntityEvent, EntityPriorityMut, EntityPriorityRef, GlobalEntity,
    GlobalEntityMap, GlobalEntitySpawner, GlobalPriorityState, GlobalRequestId, GlobalResponseId,
    OutgoingPriorityHook, UserPriorityState,
    GlobalWorldManagerType, HostType, Instant, Message, MessageContainer, MessageKinds, PacketType,
    Protocol, Replicate, ReplicatedComponent, Request, ResourceAlreadyExists, ResourceRegistry,
    Response, ResponseReceiveKey, ResponseSendKey, Serde, SerdeErr, SharedGlobalWorldManager,
    StandardHeader, Tick, Timer, WorldMutType, WorldRefType,
};

use crate::{
    connection::{connection::Connection, io::Io, tick_buffer_messages::TickBufferMessages},
    events::{world_events::WorldEvents, TickEvents},
    handshake::HandshakeManager,
    request::{GlobalRequestManager, GlobalResponseManager},
    room::Room,
    server::scope_checks_cache::ScopeChecksCache,
    time_manager::TimeManager,
    transport::{PacketReceiver, PacketSender},
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        entity_room_map::EntityRoomMap, entity_scope_map::EntityScopeMap,
        global_world_manager::GlobalWorldManager, server_auth_handler::AuthOwner,
    },
    NaiaServerError, Publicity, ReplicationConfig, RoomKey, RoomMut, RoomRef, ScopeExit,
    ServerConfig, UserKey, UserMut, UserRef, UserScopeMut, UserScopeRef, WorldUser,
};

use super::{room_store::RoomStore, scope_change::ScopeChange, user_store::UserStore};

cfg_if! {
    if #[cfg(feature = "e2e_debug")] {
        use std::sync::atomic::{AtomicUsize, Ordering};
    }
}

#[cfg(feature = "e2e_debug")]
pub static SERVER_RX_FRAMES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_TX_FRAMES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SPAWN_APPLIED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SEND_ALL_PACKETS_CALLS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_OUTGOING_CMDS_DRAINED_TOTAL: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_AUTH_GRANTED_EMITTED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_ROOM_MOVE_CALLED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SCOPE_DIFF_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_SET_AUTH_ENQUEUED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_MSGS_DRAINED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WROTE_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static SERVER_WORLD_PKTS_SENT: AtomicUsize = AtomicUsize::new(0);

/// Adapter that bridges the `OutgoingPriorityHook` trait (keyed by
/// `GlobalEntity`) to the per-user `UserPriorityState<E>` plus the read-only
/// `GlobalPriorityState<E>` layer. Constructed per-connection inside
/// `send_all_packets` from split-borrowed disjoint fields on `WorldServer`.
///
/// `advance` returns `effective_gain = global.gain × user.gain` (defaults 1.0)
/// added cumulatively into the user-layer accumulator — the canonical rule
/// from PRIORITY_ACCUMULATOR_PLAN.md III.7.1.
struct WorldServerPriorityHook<'a, E: Copy + Eq + Hash + Send + Sync> {
    global: &'a GlobalPriorityState<E>,
    user: &'a mut UserPriorityState<E>,
    converter: &'a GlobalEntityMap<E>,
}

impl<'a, E: Copy + Eq + Hash + Send + Sync> OutgoingPriorityHook
    for WorldServerPriorityHook<'a, E>
{
    fn advance(&mut self, entity: &GlobalEntity) -> f32 {
        let Ok(world_entity) = self.converter.global_entity_to_entity(entity) else {
            return 0.0;
        };
        let g = self.global.gain_override(&world_entity).unwrap_or(1.0);
        let u = self.user.gain_override(&world_entity).unwrap_or(1.0);
        self.user.advance(world_entity, g * u)
    }

    fn reset_after_send(&mut self, entity: &GlobalEntity, current_tick: u32) {
        let Ok(world_entity) = self.converter.global_entity_to_entity(entity) else {
            return;
        };
        self.user.reset_after_send(&world_entity, current_tick);
    }
}

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
pub struct WorldServer<E: Copy + Eq + Hash + Send + Sync> {
    server_config: ServerConfig,

    // Protocol
    channel_kinds: ChannelKinds,
    message_kinds: MessageKinds,
    component_kinds: ComponentKinds,
    client_authoritative_entities: bool,
    io: Io,
    // cont
    heartbeat_timer: Timer,
    ping_timer: Timer,
    timeout_timer: Timer,
    // Users
    user_store: UserStore,
    user_connections: HashMap<SocketAddr, Connection>,
    // Rooms
    room_store: RoomStore,
    // Entities
    entity_room_map: EntityRoomMap,
    entity_scope_map: EntityScopeMap,
    global_world_manager: GlobalWorldManager,
    global_entity_map: GlobalEntityMap<E>,
    // Events
    addrs_with_new_packets: HashSet<SocketAddr>,
    outstanding_disconnects: Vec<(UserKey, DisconnectReason)>,
    incoming_world_events: WorldEvents<E>,
    incoming_tick_events: TickEvents,
    // Requests/Responses
    global_request_manager: GlobalRequestManager,
    global_response_manager: GlobalResponseManager,
    // Ticks
    time_manager: TimeManager,
    // Deferred auth grants (one-tick delay to ensure entity registration)
    pending_auth_grants: Vec<(UserKey, GlobalEntity, EntityAuthStatus)>,
    scope_change_queue: VecDeque<ScopeChange>,
    // Sender-wide priority layer. Per-user layer stored here keyed by UserKey
    // — see `user_priorities`. Evicted on entity despawn.
    global_priority: GlobalPriorityState<E>,
    // Per-user priority layer. Each user has its own UserPriorityState.
    // Entries evicted on scope exit for that user; whole map entry dropped
    // when the user disconnects.
    user_priorities: HashMap<UserKey, UserPriorityState<E>>,
    // Push-based mirror of the (room, user, entity) tuples returned by
    // `scope_checks_pending()`. Maintained on room/user/entity churn; reads are
    // O(1) and zero-allocation.
    scope_checks_cache: ScopeChecksCache<E>,
    // Replicated Resources — per-World TypeId<R> ↔ GlobalEntity registry.
    // Resources are 1-component entities that auto-include into every
    // user's scope. See `_AGENTS/RESOURCES_PLAN.md`.
    resource_registry: ResourceRegistry,
    // Optional lag-compensation snapshot buffer. None until enable_historian()
    // is called; record_historian_tick() is a no-op when None.
    historian: Option<crate::historian::Historian>,
}


impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
    /// Create a new WorldServer
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let protocol: Protocol = protocol.into();

        let Protocol {
            channel_kinds,
            message_kinds,
            component_kinds,
            tick_interval,
            compression,
            client_authoritative_entities,
            ..
        } = protocol;

        let heartbeat_timer = Timer::new(server_config.connection.heartbeat_interval);
        let ping_timer = Timer::new(server_config.ping.ping_interval);
        let timeout_timer = Timer::new(server_config.connection.disconnection_timeout_duration);

        let io = Io::new(
            &server_config.connection.bandwidth_measure_duration,
            &compression,
        );

        let time_manager = TimeManager::new(tick_interval);

        // Print protocol ID for SetAuthority at startup

        Self {
            // Config
            server_config,
            channel_kinds,
            message_kinds,
            component_kinds,
            client_authoritative_entities,
            io,
            heartbeat_timer,
            ping_timer,
            timeout_timer,
            // Users
            user_store: UserStore::new(),
            user_connections: HashMap::new(),
            // Rooms
            room_store: RoomStore::new(),
            // Entities
            entity_room_map: EntityRoomMap::new(),
            entity_scope_map: EntityScopeMap::new(),
            global_world_manager: GlobalWorldManager::new(),
            global_entity_map: GlobalEntityMap::new(),
            // Events
            addrs_with_new_packets: HashSet::new(),
            outstanding_disconnects: Vec::new(), // (UserKey, DisconnectReason)
            incoming_world_events: WorldEvents::new(),
            incoming_tick_events: TickEvents::new(),
            // Requests/Responses
            global_request_manager: GlobalRequestManager::new(),
            global_response_manager: GlobalResponseManager::new(),
            time_manager,
            // Deferred auth grants
            pending_auth_grants: Vec::new(),
            scope_change_queue: VecDeque::new(),
            global_priority: GlobalPriorityState::new(),
            user_priorities: HashMap::new(),
            scope_checks_cache: ScopeChecksCache::new(),
            resource_registry: ResourceRegistry::new(),
            historian: None,
        }
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.io.is_loaded()
    }

    pub(crate) fn entity_converter(&self) -> &dyn EntityAndGlobalEntityConverter<E> {
        &self.global_entity_map
    }

    /// Attaches external sender/receiver I/O handles (used by adapter crates and test harnesses).
    pub fn io_load(&mut self, sender: Box<dyn PacketSender>, receiver: Box<dyn PacketReceiver>) {
        self.io.load(sender, receiver);
    }

    /// Registers a newly-accepted user so the world server can track their scope (adapter use only).
    pub fn receive_user(&mut self, user_key: UserKey, user_addr: SocketAddr) {
        self.user_store.insert(user_key, WorldUser::new(user_addr));
        self.user_store.register_disconnected(user_addr, user_key);
        // Auto-include of Replicated Resources happens in
        // `finalize_connection` — that's the point at which a Connection
        // exists in `user_connections` (required by `apply_scope_for_user`
        // to actually push spawn messages).
    }

    fn finalize_connection(&mut self, user_key: &UserKey, user_address: &SocketAddr) {
        if !self.user_store.contains(user_key) {
            warn!("unknown user is finalizing connection...");
            return;
        };

        use std::collections::hash_map::Entry;
        let new_connection = Connection::new(
            &self.server_config.connection,
            &self.server_config.ping,
            user_address,
            user_key,
            &self.channel_kinds,
            &self.global_world_manager,
        );

        match self.user_connections.entry(*user_address) {
            Entry::Vacant(v) => {
                v.insert(new_connection);
            }
            Entry::Occupied(mut o) => {
                o.insert(new_connection);
            }
        }

        if self.io.bandwidth_monitor_enabled() {
            self.io.register_client(user_address);
        }

        // Replicated Resources auto-scope: now that the connection
        // exists in `user_connections`, scope-include every currently-
        // existing resource entity for this user. Without this step,
        // late-joining clients never receive resources (the room gate
        // bypass in `apply_scope_for_user` requires a Connection to
        // exist; resource entities themselves never enter rooms).
        let resource_entities = self.resource_entities();
        for world_entity in resource_entities {
            self.user_scope_set_entity(user_key, &world_entity, true);
        }

        self.incoming_world_events.push_connection(user_key);
    }

    /// Maintain connection with a client and read all incoming packet data
    pub fn receive_all_packets(&mut self) {
        // Tick bandwidth monitors to clear expired packets
        self.io.tick_bandwidth_monitors();

        self.handle_disconnects();
        self.handle_pings();
        self.handle_heartbeats();
        self.handle_empty_acks();

        let mut received_addresses = HashSet::new();

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

                    received_addresses.insert(address);

                    match header.packet_type {
                        PacketType::Data => {
                            self.addrs_with_new_packets.insert(address);

                            if self
                                .read_data_packet(&address, &header, &mut reader)
                                .is_err()
                            {
                                warn!("Server Error: cannot read malformed packet");
                                continue;
                            }
                        }
                        PacketType::Heartbeat => {
                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                            }

                            continue;
                        }
                        PacketType::Ping => {
                            let response = self.time_manager.process_ping(&mut reader).unwrap();
                            // send packet
                            if self.io.send_packet(&address, response.to_packet()).is_err() {
                                // Pong send failure is transient: client will re-ping on its
                                // own timer. Persistent link failures show up via timeout.
                                warn!("Server Error: Cannot send pong packet to {}", address);
                                continue;
                            };

                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                            }

                            continue;
                        }
                        PacketType::Pong => {
                            if let Some(connection) = self.user_connections.get_mut(&address) {
                                connection.process_incoming_header(&header);
                                connection
                                    .ping_manager
                                    .process_pong(&self.time_manager, &mut reader);
                            }

                            continue;
                        }
                        PacketType::Handshake => {
                            let handshake_header_result = HandshakeHeader::de(&mut reader);
                            let Ok(HandshakeHeader::ClientConnectRequest) = handshake_header_result
                            else {
                                warn!(
                                    "Server Error: received invalid handshake packet: {:?}",
                                    handshake_header_result
                                );
                                continue;
                            };
                            let has_connection = self.user_connections.contains_key(&address);
                            if !has_connection {
                                let Some(user_key) = self.user_store.take_disconnected(&address)
                                else {
                                    warn!("Server Error: received handshake packet from unknown address: {:?}", address);
                                    continue;
                                };
                                self.finalize_connection(&user_key, &address);
                            }

                            // Send Connect Response
                            let packet = HandshakeManager::write_connect_response().to_packet();
                            if self.io.send_packet(&address, packet).is_err() {
                                warn!(
                                    "Server Error: Cannot send handshake response to {}",
                                    address
                                );
                                continue;
                            }

                            continue;
                        }
                    }
                }
                Ok(None) => {
                    // No more packets, break loop
                    break;
                }
                Err(error) => {
                    self.incoming_world_events
                        .push_error(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }

        for address in received_addresses {
            if let Some(connection) = self.user_connections.get_mut(&address) {
                connection.process_received_commands();
            }
        }
    }

    /// Decodes and applies all buffered incoming packets for this frame.
    pub fn process_all_packets<W: WorldMutType<E>>(&mut self, mut world: W, now: &Instant) {
        self.process_disconnects(&mut world);

        let addresses = std::mem::take(&mut self.addrs_with_new_packets);
        for address in addresses {
            self.process_packets(&address, &mut world, now);
        }
    }

    /// Drains and returns all pending world events for this frame.
    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        std::mem::replace(&mut self.incoming_world_events, WorldEvents::<E>::new())
    }

    /// Advances the tick clock and returns any new tick events for this frame.
    pub fn take_tick_events(&mut self, now: &Instant) -> TickEvents {
        // tick event
        if self.time_manager.recv_server_tick(now) {
            self.incoming_tick_events
                .push_tick(self.time_manager.current_tick());
        }
        std::mem::replace(&mut self.incoming_tick_events, TickEvents::new())
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) -> Result<(), NaiaServerError> {
        let container = MessageContainer::new(M::clone_box(message));
        self.send_message_inner(user_key, &ChannelKind::of::<C>(), container)
    }

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    fn send_message_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) -> Result<(), NaiaServerError> {
        let channel_settings = self.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_client() {
            panic!("Cannot send message to Client on this Channel");
        }

        let Some(user) = self.user_store.get(user_key) else {
            return Err(NaiaServerError::UserNotFound);
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            return Err(NaiaServerError::UserNotFound);
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);
        let accepted = connection.base.message_manager.send_message(
            &self.message_kinds,
            &mut converter,
            channel_kind,
            message,
        );
        if accepted { Ok(()) } else { Err(NaiaServerError::MessageQueueFull) }
    }

    /// Sends a message to all connected users using the given channel.
    ///
    /// Per-user send failures are silently discarded. If a particular user's
    /// send fails (e.g. their connection was just dropped), the error is ignored
    /// and the remaining users still receive the message. Callers that need
    /// per-user delivery guarantees should use `send_message` in a loop.
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = M::clone_box(message);
        self.broadcast_message_inner(&ChannelKind::of::<C>(), cloned_message);
    }

    fn broadcast_message_inner(
        &mut self,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        // Wrap once in Arc — each per-user clone is a refcount increment, not
        // a heap allocation. At 1,262 CCU this drops from 1,262 clone_box()
        // allocations per broadcast to 1.
        let container = MessageContainer::new(message_box);
        let user_keys: Vec<UserKey> = self.user_keys().to_vec();
        for user_key in user_keys {
            let _ = self.send_message_inner(&user_key, channel_kind, container.clone());
        }
    }

    /// Sends a typed request to the given user and returns a key for receiving the response.
    pub fn send_request<C: Channel, Q: Request>(
        &mut self,
        user_key: &UserKey,
        request: &Q,
    ) -> Result<ResponseReceiveKey<Q::Response>, NaiaServerError> {
        let cloned_request = Q::clone_box(request);
        let id = self.send_request_inner(user_key, &ChannelKind::of::<C>(), cloned_request)?;
        Ok(ResponseReceiveKey::new(id))
    }

    fn send_request_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        request_box: Box<dyn Message>,
    ) -> Result<GlobalRequestId, NaiaServerError> {
        let channel_settings = self.channel_kinds.channel(channel_kind);

        if !channel_settings.can_request_and_respond() {
            panic!("Requests can only be sent over Bidirectional, Reliable Channels");
        }

        let request_id = self.global_request_manager.create_request_id(user_key);

        let Some(user) = self.user_store.get(user_key) else {
            warn!("user does not exist");
            return Err(NaiaServerError::Message("user does not exist".to_string()));
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            warn!("currently not connected to user");
            return Err(NaiaServerError::Message(
                "currently not connected to user".to_string(),
            ));
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);

        let message = MessageContainer::new(request_box);
        connection.base.message_manager.send_request(
            &self.message_kinds,
            &mut converter,
            channel_kind,
            request_id,
            message,
        );

        Ok(request_id)
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
        let Some((user_key, channel_kind, local_response_id)) = self
            .global_response_manager
            .destroy_response_id(response_id)
        else {
            return false;
        };
        let Some(user) = self.user_store.get(&user_key) else {
            return false;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            return false;
        };
        let mut converter = connection
            .base
            .world_manager
            .entity_converter_mut(&self.global_world_manager);
        let response = MessageContainer::new(response_box);
        connection.base.message_manager.send_response(
            &self.message_kinds,
            &mut converter,
            &channel_kind,
            local_response_id,
            response,
        );
        true
    }

    /// Polls for a response to a previously sent request; returns `None` if not yet received.
    pub fn receive_response<S: Response>(
        &mut self,
        response_key: &ResponseReceiveKey<S>,
    ) -> Option<(UserKey, S)> {
        let request_id = response_key.request_id();
        let (user_key, container) = self.global_request_manager.destroy_request_id(&request_id)?;
        let response: S = Box::<dyn Any + 'static>::downcast::<S>(container.to_boxed_any())
            .ok()
            .map(|boxed_s| *boxed_s)
            .unwrap();
        Some((user_key, response))
    }
    /// Drains and returns all tick-buffered messages sent by clients for the given tick.
    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        let mut tick_buffer_messages = TickBufferMessages::new();
        for (_user_address, connection) in self.user_connections.iter_mut() {
            // receive messages from anyone
            connection.tick_buffer_messages(tick, &mut tick_buffer_messages);
        }
        tick_buffer_messages
    }

    // Updates

    /// Returns every `(room, user, entity)` tuple that currently exists —
    /// i.e. every entity in a room, crossed with every user in that room.
    /// Returns only `(room, user, entity)` tuples added since the last call to
    /// `mark_scope_checks_pending_handled()`. After initial entity/user load
    /// the returned Vec is empty every tick — zero allocation, zero iteration.
    ///
    /// Use this for incremental scope evaluation ("add every new entity once").
    /// Call `mark_scope_checks_pending_handled()` after processing each batch.
    ///
    /// For a full re-evaluation of all current pairs (e.g. at startup, or after
    /// a bulk teleport), call `mark_all_scope_checks_pending()` first to
    /// enqueue the full cross-product into the pending queue.
    pub fn scope_checks_pending(&self) -> Vec<(RoomKey, UserKey, E)> {
        self.scope_checks_cache.pending_slice().to_vec()
    }

    /// Clears the pending queue. Call after processing `scope_checks_pending()`.
    pub fn mark_scope_checks_pending_handled(&mut self) {
        self.scope_checks_cache.mark_pending_handled();
    }

    /// Re-enqueues all current (room, user, entity) tuples into the pending
    /// queue. Use this to force a full scope re-evaluation (e.g. at server
    /// startup, or after bulk world changes) without bypassing the incremental
    /// system. Follow with `scope_checks_pending()` + `mark_scope_checks_pending_handled()`.
    pub fn mark_all_scope_checks_pending(&mut self) {
        self.scope_checks_cache.mark_all_pending();
    }

    /// Slow-path recompute — used by tests to verify the cache stays
    /// in sync with `(rooms × users × entities)` truth.
    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub fn send_all_packets<W: WorldRefType<E>>(&mut self, world: W) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_SEND_ALL_PACKETS_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        let now = Instant::now();

        // Zero per-tick byte counter so outgoing_bytes_last_tick() reports
        // only the bytes sent during THIS tick (readable after send_packets).
        self.io.reset_outgoing_bytes_this_tick();

        // update entity scopes
        self.update_entity_scopes(&world);

        // loop through all connections, send packet
        let mut user_addresses: Vec<SocketAddr> = self.user_connections.keys().copied().collect();
        // shuffle order of connections in order to avoid priority among users
        fastrand::shuffle(&mut user_addresses);

        for user_address in user_addresses {
            let connection = self.user_connections.get_mut(&user_address).unwrap();
            // Build a per-user priority hook over the (global, user) layers.
            // `global` provides the read-only `gain_override`; `user` is
            // mutated by `advance` / `reset_after_send`. Split-borrow is safe
            // because `user_priorities` and `global_priority` are disjoint
            // fields on `WorldServer`.
            let user_layer = self
                .user_priorities
                .entry(connection.user_key)
                .or_default();
            let mut hook = WorldServerPriorityHook {
                global: &self.global_priority,
                user: user_layer,
                converter: &self.global_entity_map,
            };
            connection.send_packets(
                &self.channel_kinds,
                &self.message_kinds,
                &self.component_kinds,
                &now,
                &mut self.io,
                &world,
                &self.global_entity_map,
                &self.global_world_manager,
                &self.time_manager,
                &mut hook,
            );
        }

        // Flush deferred auth grants (one-tick delay ensures entity registration on client)
        let pending_grants = std::mem::take(&mut self.pending_auth_grants);
        for (owner_user_key, global_entity, _granted_status) in pending_grants {
            // Collect addresses first to avoid borrowing issues
            let user_addresses: Vec<SocketAddr> = self.user_connections.keys().copied().collect();
            // Send SetAuthority to all users in scope (canonical path)
            for address in user_addresses {
                let Some(conn) = self.user_connections.get_mut(&address) else {
                    continue;
                };
                if !conn.base.world_manager.has_global_entity(&global_entity) {
                    continue;
                }
                let user_key_for_conn = conn.user_key;
                let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
                if owner_user_key == user_key_for_conn {
                    new_status = EntityAuthStatus::Granted;
                }
                // Use host_send_set_auth which handles both HostEntity and RemoteEntity
                conn.base
                    .world_manager
                    .host_send_set_auth(&global_entity, new_status);
                #[cfg(feature = "e2e_debug")]
                if new_status == EntityAuthStatus::Granted {
                    SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                    SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&'_ mut self, mut world: W) -> EntityMut<'_, E, W> {
        let world_entity = world.spawn_entity();

        self.spawn_entity_inner(&world_entity);

        EntityMut::new(self, world, &world_entity)
    }

    /// Creates a new Entity with a specific id
    fn spawn_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self.global_entity_map.spawn(*world_entity, None);
        self.global_world_manager
            .insert_entity_record(&global_entity, EntityOwner::Server);
    }

    fn spawn_static_entity_inner(&mut self, world_entity: &E) {
        let global_entity = self.global_entity_map.spawn(*world_entity, None);
        self.global_world_manager
            .insert_static_entity_record(&global_entity, EntityOwner::Server);
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn enable_entity_replication(&mut self, entity: &E) {
        self.spawn_entity_inner(entity);
    }

    /// Bevy adapter crates only: register an already-spawned Bevy entity as a
    /// static (immutable) naia entity. Static entities are never diff-tracked
    /// after initial replication. Post-spawn mutation panics via EntityMut.
    pub fn enable_static_entity_replication(&mut self, entity: &E) {
        self.spawn_static_entity_inner(entity);
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn disable_entity_replication(&mut self, world_entity: &E) {
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(world_entity);
    }

    /// Pauses replication for this entity: component changes are no longer
    /// transmitted to any client until `resume_entity_replication` is called.
    /// The entity remains spawned on clients; it simply stops receiving updates.
    ///
    /// # Adapter use only
    pub fn pause_entity_replication(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
        else {
            warn!("pause_entity_replication: entity not found in global map");
            return;
        };
        self.global_world_manager
            .pause_entity_replication(&global_entity);
    }

    /// Resumes replication for an entity previously paused with
    /// `pause_entity_replication`. Component changes will again be tracked and
    /// transmitted to clients on the next send tick.
    ///
    /// # Adapter use only
    pub fn resume_entity_replication(&mut self, world_entity: &E) {
        let Ok(global_entity) = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
        else {
            warn!("resume_entity_replication: entity not found in global map");
            return;
        };
        self.global_world_manager
            .resume_entity_replication(&global_entity);
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn set_global_entity_counter_for_test(&mut self, value: u64) {
        self.global_entity_map
            .set_global_entity_counter_for_test(value);
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn inject_tick_buffer_message<C: Channel, M: Message>(
        &mut self,
        user_key: &UserKey,
        host_tick: &Tick,
        message_tick: &Tick,
        message: &M,
    ) -> bool {
        let channel_kind = ChannelKind::of::<C>();
        let message_box = M::clone_box(message);
        let container = MessageContainer::new(message_box);
        let Some(user) = self.user_store.get(user_key) else {
            warn!("inject_tick_buffer_message: user {:?} does not exist", user_key);
            return false;
        };
        let address = user.address();
        let Some(connection) = self.user_connections.get_mut(&address) else {
            warn!("inject_tick_buffer_message: no connection for user {:?}", user_key);
            return false;
        };
        connection.inject_tick_buffer_message(&channel_kind, host_tick, message_tick, container)
    }

    /// Returns `true` if the entity has been marked as static (never re-sent after initial spawn).
    pub fn entity_is_static(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return false;
        };
        self.global_world_manager.entity_is_static(&global_entity)
    }

    /// Marks an entity as static; its component data will not be re-sent after the initial spawn packet.
    pub fn mark_entity_as_static(&mut self, world_entity: &E) {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            panic!("entity not found in global map");
        };
        self.global_world_manager.mark_entity_as_static(&global_entity);
    }

    /// Returns `true` if the entity is currently in `Delegated` replication mode.
    pub fn entity_is_delegated(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return false;
        };
        self.global_world_manager.entity_is_delegated(&global_entity)
    }

    // ========================================================================
    // Replicated Resources
    // ========================================================================
    //
    // A Replicated Resource is internally a hidden 1-component entity that:
    //   - Is registered in the per-world `ResourceRegistry` keyed by `R`'s
    //     TypeId, allowing O(1) `resource_entity::<R>()` lookups.
    //   - Is auto-included in every connected user's scope (so resources
    //     reach every client without explicit room/scope work).
    //   - Otherwise reuses the existing entity replication pipeline 100%
    //     (spawn/update/despawn, per-field diff tracking, authority).
    //
    // See `_AGENTS/RESOURCES_PLAN.md`.

    /// Insert a Replicated Resource using a dynamic entity ID.
    ///
    /// Spawns the hidden entity, attaches `value` as its sole replicated
    /// component, registers it in the per-world `ResourceRegistry`, and
    /// auto-includes it in every currently-connected user's scope.
    ///
    /// Returns the underlying world-entity handle for tests / advanced use.
    /// Bevy adapter callers will not usually surface this entity to user
    /// code (resources are entity-less from the user's POV).
    ///
    /// Errors with `ResourceAlreadyExists` if `R` was already inserted
    /// in this world. The world remains unchanged on error.
    /// Insert a Replicated Resource.
    ///
    /// Pass `is_static = true` for long-lived singletons that never change
    /// after insertion (no diff-tracking on the wire). Pass `false` for
    /// resources whose fields are updated over time (delta-tracked).
    ///
    /// Errors with `ResourceAlreadyExists` if `R` was already inserted.
    /// The world remains unchanged on error.
    pub fn insert_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        mut world: W,
        value: R,
        is_static: bool,
    ) -> Result<E, ResourceAlreadyExists> {
        let world_entity = world.spawn_entity();
        if is_static {
            self.spawn_static_entity_inner(&world_entity);
        } else {
            self.spawn_entity_inner(&world_entity);
        }
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(&world_entity)
            .expect("entity just spawned must be in global map");

        if let Err(e) = self.resource_registry.insert::<R>(global_entity) {
            self.despawn_entity_worldless(&world_entity);
            world.despawn_entity(&world_entity);
            return Err(e);
        }

        self.insert_component(&mut world, &world_entity, value);

        let user_keys: Vec<UserKey> = self.user_store.keys_copied();
        for user_key in user_keys {
            self.user_scope_set_entity(&user_key, &world_entity, true);
        }

        Ok(world_entity)
    }

    /// Remove the resource of type `R` if present. Despawns the hidden
    /// entity (which propagates a despawn to every client where it was
    /// in scope) and clears the registry entries on both sides.
    ///
    /// Returns `true` if a resource was removed, `false` if `R` was not
    /// present.
    pub fn remove_resource<W: WorldMutType<E>, R: ReplicatedComponent>(
        &mut self,
        mut world: W,
    ) -> bool {
        let Some(global_entity) = self.resource_registry.remove::<R>() else {
            return false;
        };
        let world_entity = match self
            .global_entity_map
            .global_entity_to_entity(&global_entity)
        {
            Ok(e) => e,
            Err(_) => return true, // registry stale; nothing more to do
        };
        // Despawn from inner tracking (scope, priority, replication state)
        self.despawn_entity_worldless(&world_entity);
        // Then despawn from the world itself.
        world.despawn_entity(&world_entity);
        true
    }

    /// O(1): the hidden entity carrying resource `R`, or `None` if
    /// `R` is not currently inserted.
    pub fn resource_entity<R: ReplicatedComponent>(&self) -> Option<E> {
        let global_entity = self.resource_registry.entity_for::<R>()?;
        self.global_entity_map
            .global_entity_to_entity(&global_entity)
            .ok()
    }

    /// O(1): is `world_entity` a hidden resource entity?
    /// Used by Bevy adapter event-emission filter (D13) to suppress
    /// SpawnEntityEvent / component events for resource entities.
    pub fn is_resource_entity(&self, world_entity: &E) -> bool {
        let Ok(global_entity) = self.global_entity_map.entity_to_global_entity(world_entity) else {
            return false;
        };
        self.resource_registry.is_resource_entity(&global_entity)
    }

    /// True iff a resource of type `R` is currently inserted.
    pub fn has_resource<R: ReplicatedComponent>(&self) -> bool {
        self.resource_registry.entity_for::<R>().is_some()
    }

    /// Number of currently-inserted resources.
    pub fn resources_count(&self) -> usize {
        self.resource_registry.len()
    }

    /// Read-only handle to the per-resource priority state.
    /// Returns `None` if the resource is not currently inserted.
    /// Per D9 / §4.4 of RESOURCES_PLAN: per-resource priority is just
    /// per-entity priority on the hidden resource entity. Default gain
    /// is 1.0 (same as any entity); no special "Resource" priority tier.
    pub fn resource_priority<R: ReplicatedComponent>(&self) -> Option<EntityPriorityRef<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority(entity))
    }

    /// Mutable handle to the per-resource priority state.
    /// Returns `None` if the resource is not currently inserted.
    /// User can call `.set_gain(f32)` to tune priority or `.boost_once(f32)`
    /// for a one-shot bump.
    pub fn resource_priority_mut<R: ReplicatedComponent>(
        &mut self,
    ) -> Option<EntityPriorityMut<'_, E>> {
        let entity = self.resource_entity::<R>()?;
        Some(self.global_entity_priority_mut(entity))
    }

    /// Server-side authority status for resource `R`. Returns `None`
    /// if `R` is not currently inserted or if the resource is not
    /// configured for delegation.
    pub fn resource_authority_status<R: ReplicatedComponent>(
        &self,
    ) -> Option<EntityAuthStatus> {
        let entity = self.resource_entity::<R>()?;
        self.entity_authority_status(&entity)
    }

    /// Iterate over the hidden entities of all currently-inserted resources.
    /// Used by the connect-flow to auto-include all resources in a new
    /// user's scope.
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

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_replication_config(&self, world_entity: &E) -> Option<ReplicationConfig> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.global_world_manager
            .entity_replication_config(&global_entity)
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_take_authority(&mut self, world_entity: &E) -> Result<(), AuthorityError> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .global_world_manager
            .server_take_authority(&global_entity);

        if let Ok(previous_owner) = result {
            // When server takes authority, send Denied to clients whose state will change:
            // - If there was a client holder (Granted→Denied): send only to that client
            // - If no holder (Available→Denied): send to all clients in scope
            self.send_take_authority_messages(&global_entity, previous_owner);
            self.incoming_world_events.push_auth_reset(world_entity);
        }
        result.map(|_| ())
    }

    fn send_take_authority_messages(
        &mut self,
        global_entity: &GlobalEntity,
        previous_owner: AuthOwner,
    ) {
        // Server has taken authority - send appropriate messages based on previous state
        match previous_owner {
            AuthOwner::Client(prev_holder_key) => {
                // There was a client holder - only they need to transition (Granted→Denied)
                // Other clients were already Denied, no message needed
                if let Some(user) = self.user_store.get(&prev_holder_key) {
                    if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                        if connection
                            .base
                            .world_manager
                            .has_global_entity(global_entity)
                        {
                            connection
                                .base
                                .world_manager
                                .host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                        }
                    }
                }
            }
            AuthOwner::None => {
                // No holder - all clients were Available, all need to transition to Denied
                for (_user_key, user) in self.user_store.iter() {
                    if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                        if !connection
                            .base
                            .world_manager
                            .has_global_entity(global_entity)
                        {
                            continue;
                        }
                        connection
                            .base
                            .world_manager
                            .host_send_set_auth(global_entity, EntityAuthStatus::Denied);
                    }
                }
            }
            AuthOwner::Server => {
                // Server already had authority - no change needed
            }
        }
    }

    fn send_reset_authority_messages(&mut self, global_entity: &GlobalEntity) {
        // authority was released from entity
        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_user_key, user) in self.user_store.iter() {
            if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                // Check if entity exists on the client (as either HostEntity or RemoteEntity)
                // After migration, the entity is a RemoteEntity on the client, but the server
                // still sends from HostEntity perspective and the client's routing handles it
                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not mapped to this connection
                    continue;
                }

                // Send UpdateAuthority action through EntityActionEvent system
                // The server always sends from HostEntity perspective, and the client's
                // routing logic will handle converting it to the correct entity type
                connection
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, EntityAuthStatus::Available);
            }
        }
    }

    /// Applies a new [`ReplicationConfig`] to an entity, changing its visibility and authority model.
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        config: ReplicationConfig,
    ) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self.global_world_manager.has_entity(&global_entity) {
            panic!("Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Server, before configuring replication.");
        }
        let entity_owner = self
            .global_world_manager
            .entity_owner(&global_entity)
            .unwrap();
        let server_owned: bool = entity_owner.is_server();
        let client_owned: bool = entity_owner.is_client();
        // When the server initiates delegation on a client-owned entity
        // (per spec [entity-ownership-11]), `entity_enable_delegation` needs
        // the owning client's key as `client_origin` so the migration flow
        // runs (`enable_delegation_client_owned_entity`) AND so the owning
        // client doesn't receive an EnableDelegation message it can't
        // route — its `HostEntityChannel::process_messages` would panic
        // with "unexpected message type: EnableDelegation".
        let client_origin: Option<UserKey> = match entity_owner {
            EntityOwner::Client(uk)
            | EntityOwner::ClientPublic(uk)
            | EntityOwner::ClientWaiting(uk) => Some(uk),
            EntityOwner::Server | EntityOwner::Local => None,
        };
        let prev_config = self
            .global_world_manager
            .entity_replication_config(&global_entity)
            .unwrap();
        if prev_config == config {
            // Fully identical — no-op
            return;
        }

        // Handle publicity state machine only when publicity changed
        if prev_config.publicity != config.publicity {
            match prev_config.publicity {
                Publicity::Private => {
                    if server_owned {
                        panic!("Server-owned entity should never be private");
                    }
                    match config.publicity {
                        Publicity::Private => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                        Publicity::Public => {
                            // private -> public
                            self.publish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Delegated => {
                            // private -> delegated
                            // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                            // which transfers ownership to server
                            self.publish_entity(world, &global_entity, world_entity, true);
                            self.entity_enable_delegation(
                                world,
                                &global_entity,
                                world_entity,
                                client_origin,
                            );
                        }
                    }
                }
                Publicity::Public => {
                    match config.publicity {
                        Publicity::Private => {
                            // public -> private
                            if server_owned {
                                panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                            }
                            self.unpublish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Public => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                        Publicity::Delegated => {
                            // public -> delegated
                            // Per spec [entity-ownership-11], server CAN enable delegation on client-owned entities,
                            // which transfers ownership to server
                            self.entity_enable_delegation(
                                world,
                                &global_entity,
                                world_entity,
                                client_origin,
                            );
                        }
                    }
                }
                Publicity::Delegated => {
                    if client_owned {
                        panic!("Client-owned entity should never be delegated");
                    }
                    match config.publicity {
                        Publicity::Private => {
                            // delegated -> private
                            if server_owned {
                                panic!("Cannot unpublish a Server-owned Entity (doing so would disable replication entirely, just use a local entity instead)");
                            }
                            self.entity_disable_delegation(world, &global_entity, world_entity);
                            self.unpublish_entity(world, &global_entity, world_entity, true);
                        }
                        Publicity::Public => {
                            // delegated -> public
                            self.entity_disable_delegation(world, &global_entity, world_entity);
                        }
                        Publicity::Delegated => {
                            unreachable!("publicity prev == next but outer check passed");
                        }
                    }
                }
            }
        }

        // Always persist the scope_exit field regardless of whether publicity changed
        self.global_world_manager
            .entity_set_scope_exit(&global_entity, config.scope_exit);
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_give_authority(
        &mut self,
        origin_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per contract [entity-authority-12] ("server give_authority
        // requires scope"): the target user must be able to see the
        // entity, otherwise return `NotInScope` and leave the holder
        // unchanged. Without this gate the server could silently grant
        // authority to an out-of-scope user, who would never receive the
        // matching SetAuthority message and would diverge from server
        // state.
        if !self.user_scope_has_entity(origin_user, world_entity) {
            return Err(AuthorityError::NotInScope);
        }

        // Use the server-priority give path so we override any current
        // holder (per contract [entity-authority-10]). The previous
        // `client_request_authority` path failed with NotAvailable
        // whenever the entity was already held — including by the same
        // user — which broke the "server give overrides current holder"
        // contract.
        let previous_owner = self
            .global_world_manager
            .server_give_authority_to_client(&global_entity, origin_user)?;

        // Idempotent re-give to the same user: the auth-handler already
        // returned without state change (see
        // `server_give_authority_to_client`); skip fan-out so we don't
        // drive an illegal Granted→Granted transition through the
        // per-client auth channel.
        if previous_owner == AuthOwner::Client(*origin_user) {
            return Ok(());
        }

        // entity authority was granted for origin user
        // for any users that have this entity in scope, send an `update_authority_status` message

        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (user_key, user) in self.user_store.iter() {
            let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                continue;
            };
            // Check if entity exists on the client (as either HostEntity or RemoteEntity)
            // After migration, the entity is a RemoteEntity on the client, but the server
            // still sends from HostEntity perspective and the client's routing handles it
            if !connection
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                // entity is not mapped to this connection
                continue;
            }

            let mut new_status: EntityAuthStatus = EntityAuthStatus::Denied;
            if origin_user == user_key {
                new_status = EntityAuthStatus::Granted;
            }

            // Send UpdateAuthority action through EntityActionEvent system
            // The server always sends from HostEntity perspective, and the client's
            // routing logic will handle converting it to the correct entity type
            connection
                .base
                .world_manager
                .host_send_set_auth(&global_entity, new_status);
            #[cfg(feature = "e2e_debug")]
            if new_status == EntityAuthStatus::Granted {
                SERVER_SET_AUTH_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                SERVER_AUTH_GRANTED_EMITTED.fetch_add(1, Ordering::Relaxed);
            }
        }

        // SetAuthority is sent in the per-connection loop above — do NOT also push to
        // auth_grants, which would queue a second SetAuthority send and drive illegal
        // transitions (e.g. Granted→Denied for the grantee, or Denied→Denied for observers).
        // Covered by [entity-authority-17] @Scenario(38).

        // Push to events for external systems (e.g., Bevy adapter, test harness)
        // Events are separate from network messages - they're notifications for external consumers
        self.incoming_world_events
            .push_auth_grant(origin_user, world_entity);

        Ok(())
    }

    fn entity_handle_client_request_authority(
        &mut self,
        requester_user: &UserKey,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        if !self.user_scope_has_entity(requester_user, world_entity) {
            return Err(AuthorityError::NotInScope);
        }

        let requester = AuthOwner::from_user_key(Some(requester_user));
        self.global_world_manager
            .client_request_authority(&global_entity, &requester)?;

        for (user_key, user) in self.user_store.iter() {
            let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                continue;
            };
            if !connection
                .base
                .world_manager
                .has_global_entity(&global_entity)
            {
                continue;
            }
            let new_status = if requester_user == user_key {
                EntityAuthStatus::Granted
            } else {
                EntityAuthStatus::Denied
            };
            connection
                .base
                .world_manager
                .host_send_set_auth(&global_entity, new_status);
        }

        self.incoming_world_events
            .push_auth_grant(requester_user, world_entity);

        Ok(())
    }

    fn entity_enable_delegation_response(
        &mut self,
        _user_key: &UserKey,
        _global_entity: &GlobalEntity,
    ) {
        // EnableDelegationResponse does NOT send SetAuthority messages.
        // Enabling delegation establishes the delegated-mode baseline as Available (AuthNone) for clients.
        // Any Denied/Granted status changes come ONLY from subsequent authority operations (request/give/take/release).
        // The client initializes local auth status to Available when processing EnableDelegation message,
        // so no SetAuthority message is needed here.
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub(crate) fn entity_authority_status(&self, world_entity: &E) -> Option<EntityAuthStatus> {
        let global_entity = match self.global_entity_map.entity_to_global_entity(world_entity) {
            Ok(ge) => ge,
            Err(_) => return None,
        };
        self.global_world_manager
            .entity_authority_status(&global_entity)
    }

    /// This is used only for Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(
        &mut self,
        origin_user: Option<&UserKey>,
        world_entity: &E,
    ) -> Result<(), AuthorityError> {
        let releaser = AuthOwner::from_user_key(origin_user);
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        let result = self
            .global_world_manager
            .client_release_authority(&global_entity, &releaser);
        if result.is_ok() {
            self.send_reset_authority_messages(&global_entity);
        }
        result
    }

    /// Enable delegation for a server-owned entity
    ///
    /// This enables delegation for the given entity, allowing authority to be
    /// requested/released. The entity must be server-owned and Public.
    /// Returns true if delegation was enabled, false otherwise.
    pub(crate) fn enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> bool {
        let global_entity = match self.global_entity_map.entity_to_global_entity(world_entity) {
            Ok(ge) => ge,
            Err(_) => return false,
        };

        // Only enable delegation for server-owned entities
        let owner = self.entity_owner(world_entity);
        if !owner.is_server() {
            return false;
        }

        self.entity_enable_delegation(world, &global_entity, world_entity, None);
        true
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
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
        if world.has_entity(entity) {
            return EntityMut::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        world.entities()
    }

    // This intended to be used by adapter crates
    pub(crate) fn entity_owner(&self, world_entity: &E) -> EntityOwner {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if let Some(owner) = self.global_world_manager.entity_owner(&global_entity) {
            return owner;
        }
        EntityOwner::Local
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.user_store.contains(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    ///
    /// # Panics
    /// Panics if no user exists for the given key. Prefer [`user_opt`](Self::user_opt)
    /// when calling from a context where the key may be stale (e.g., inside a
    /// disconnect handler that received a copy of the key before disconnect was processed).
    pub fn user(&'_ self, user_key: &UserKey) -> UserRef<'_, E> {
        if self.user_store.contains(user_key) {
            return UserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns `Some(UserRef)` if the user exists, or `None` if the key is stale.
    ///
    /// Use this instead of [`user`](Self::user) when you cannot guarantee the key is still live.
    pub fn user_opt(&'_ self, user_key: &UserKey) -> Option<UserRef<'_, E>> {
        if self.user_store.contains(user_key) {
            Some(UserRef::new(self, user_key))
        } else {
            None
        }
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    ///
    /// # Panics
    /// Panics if no user exists for the given key. Prefer [`user_mut_opt`](Self::user_mut_opt)
    /// when calling from a context where the key may be stale.
    pub fn user_mut(&'_ mut self, user_key: &UserKey) -> UserMut<'_, E> {
        if self.user_store.contains(user_key) {
            return UserMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns `Some(UserMut)` if the user exists, or `None` if the key is stale.
    ///
    /// Use this instead of [`user_mut`](Self::user_mut) when you cannot guarantee the key is still live.
    pub fn user_mut_opt(&'_ mut self, user_key: &UserKey) -> Option<UserMut<'_, E>> {
        if self.user_store.contains(user_key) {
            Some(UserMut::new(self, user_key))
        } else {
            None
        }
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, user) in self.user_store.iter() {
            if self.user_connections.contains_key(&user.address()) {
                output.push(*user_key);
            }
        }

        output
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.user_store.len()
    }

    /// Returns the number of users that have fully connected (handshake complete).
    pub fn user_count(&self) -> usize {
        self.user_keys().len()
    }

    /// Returns the total number of replicated entities currently tracked by the server.
    pub fn entity_count(&self) -> usize {
        self.global_entity_map.entity_count()
    }

    /// Returns a UserScopeRef, which is used to query whether a given user has
    pub fn user_scope(&'_ self, user_key: &UserKey) -> UserScopeRef<'_, E> {
        if self.user_store.contains(user_key) {
            return UserScopeRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope_mut(&'_ mut self, user_key: &UserKey) -> UserScopeMut<'_, E> {
        if self.user_store.contains(user_key) {
            return UserScopeMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    // Priority

    /// Read-only handle to the sender-wide (global) priority state for `entity`.
    /// Combined multiplicatively with the per-user gain at sort time.
    pub fn global_entity_priority(&self, entity: E) -> EntityPriorityRef<'_, E> {
        self.global_priority.get_ref(entity)
    }

    /// Mutable handle to the sender-wide (global) priority state for `entity`.
    /// Lazy-creates an entry on first write.
    pub fn global_entity_priority_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        self.global_priority.get_mut(entity)
    }

    /// Read-only handle to the per-user priority state for `entity` on the
    /// given user's connection. Evicted on scope exit for that user.
    pub fn user_entity_priority(
        &self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityRef<'_, E> {
        // Fetch this user's layer; if none exists yet, fall back to the
        // global `Ref`-on-missing semantics via a fresh empty layer.
        // Safe because `EntityPriorityRef` reads `Option<&EntityPriorityData>`
        // via the state map — no allocation is required on the read path.
        match self.user_priorities.get(user_key) {
            Some(layer) => layer.get_ref(entity),
            None => {
                // No entry exists for this user; return an empty ref by
                // peeking through an ephemeral empty layer. We use a static
                // path via a constructor that reads None for `state`.
                EntityPriorityRef::empty(entity)
            }
        }
    }

    /// Mutable handle to the per-user priority state for `entity` on the given
    /// user's connection. Lazy-creates the user's priority layer and the entity
    /// entry on first write.
    pub fn user_entity_priority_mut(
        &mut self,
        user_key: &UserKey,
        entity: E,
    ) -> EntityPriorityMut<'_, E> {
        let layer = self
            .user_priorities
            .entry(*user_key)
            .or_default();
        layer.get_mut(entity)
    }

    // Ticks

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        self.time_manager.current_tick()
    }

    /// Gets the current average tick duration of the Server
    pub fn average_tick_duration(&self) -> Duration {
        self.time_manager.average_tick_duration()
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn create_room(&'_ mut self) -> RoomMut<'_, E> {
        let new_room = Room::new();
        let room_key = self.room_store.insert(new_room);
        RoomMut::new(self, &room_key)
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.room_store.contains(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&'_ self, room_key: &RoomKey) -> RoomRef<'_, E> {
        if self.room_store.contains(room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&'_ mut self, room_key: &RoomKey) -> RoomMut<'_, E> {
        if self.room_store.contains(room_key) {
            return RoomMut::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Return a list of all the Server's Rooms' keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        self.room_store.keys()
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.room_store.len()
    }

    /// Returns the total number of rooms that currently exist.
    pub fn room_count(&self) -> usize {
        self.room_keys().len()
    }

    // Bandwidth monitoring
    /// Total outgoing bandwidth averaged over the monitor window (bytes/sec).
    pub fn outgoing_bandwidth_total(&self) -> f32 {
        self.io.outgoing_bandwidth_total()
    }

    /// Bytes sent (post-compression, pre-transport) during the most recent
    /// `send_all_packets` call. Precise, non-rolling counter. Read after a
    /// tick has run; reset to 0 at the start of the next `send_all_packets`.
    pub fn outgoing_bytes_last_tick(&self) -> u64 {
        self.io.outgoing_bytes_last_tick()
    }

    /// Total incoming bandwidth averaged over the monitor window (bytes/sec).
    pub fn incoming_bandwidth_total(&self) -> f32 {
        self.io.incoming_bandwidth_total()
    }

    /// Outgoing bandwidth to a specific client address, averaged over the monitor window (bytes/sec).
    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        self.io.outgoing_bandwidth_to_client(address)
    }

    /// Incoming bandwidth from a specific client address, averaged over the monitor window (bytes/sec).
    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        self.io.incoming_bandwidth_from_client(address)
    }

    // Ping
    /// Gets the average Round Trip Time measured to the given User's Client
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.user_store.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address()) {
                return Some(connection.ping_manager.rtt_average);
            }
        }
        None
    }

    /// Gets the average Jitter measured in connection to the given User's
    /// Client
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.user_store.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address()) {
                return Some(connection.ping_manager.jitter_average);
            }
        }
        None
    }

    // Historian — lag-compensation snapshot buffer

    /// Enable the per-tick snapshot buffer for server-side lag compensation.
    ///
    /// `max_ticks` controls how many past ticks are retained. A value of 64
    /// covers ~3 seconds at 20 Hz, which is appropriate for most games.
    /// Call once at startup; calling again replaces the buffer.
    pub fn enable_historian(&mut self, max_ticks: u16) {
        self.historian = Some(crate::historian::Historian::new(max_ticks));
    }

    /// Like `enable_historian`, but only snapshots the component kinds in
    /// `filter`. Use this to reduce per-tick clone cost when you only need
    /// a subset of components for lag-compensation (e.g. `Position`, `Health`).
    pub fn enable_historian_filtered(
        &mut self,
        max_ticks: u16,
        filter: impl IntoIterator<Item = naia_shared::ComponentKind>,
    ) {
        self.historian = Some(crate::historian::Historian::new_filtered(max_ticks, filter));
    }

    /// Record a snapshot of all replicated component values at the given tick.
    ///
    /// Call this each tick after game-state mutation and before
    /// `send_all_packets`, so the snapshot reflects authoritative state.
    /// This is a no-op if `enable_historian()` has not been called.
    pub fn record_historian_tick<W: WorldRefType<E>>(&mut self, world: W, tick: Tick) {
        if let Some(historian) = &mut self.historian {
            historian.record_tick(
                tick,
                &self.global_world_manager,
                &self.global_entity_map,
                &world,
            );
        }
    }

    /// Returns a read-only reference to the Historian, or `None` if it has not
    /// been enabled via `enable_historian()`.
    pub fn historian(&self) -> Option<&crate::historian::Historian> {
        self.historian.as_ref()
    }

    /// Returns a snapshot of per-connection diagnostics for the given user.
    ///
    /// Returns `None` if the user is not connected. All fields are rolling
    /// averages or short-window estimates computed on demand; no per-tick
    /// allocation occurs.
    pub fn connection_stats(&self, user_key: &UserKey) -> Option<ConnectionStats> {
        let user = self.user_store.get(user_key)?;
        let connection = self.user_connections.get(&user.address())?;
        let pm = &connection.ping_manager;
        Some(ConnectionStats {
            rtt_ms: pm.rtt_average,
            rtt_p50_ms: pm.rtt_p50_ms(),
            rtt_p99_ms: pm.rtt_p99_ms(),
            jitter_ms: pm.jitter_average,
            packet_loss_pct: connection.base.packet_loss_pct(),
            kbps_sent: self.io.outgoing_bandwidth_to_client(&user.address()),
            kbps_recv: self.io.incoming_bandwidth_from_client(&user.address()),
        })
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, world_entity: &E) {
        if !world.has_entity(world_entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // Delete from world
        world.despawn_entity(world_entity);

        self.despawn_entity_worldless(world_entity);
    }

    /// Removes an entity from all replication state without touching the world (adapter use only).
    pub fn despawn_entity_worldless(&mut self, world_entity: &E) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        if !self.global_world_manager.has_entity(&global_entity) {
            info!("attempting to despawn entity that does not exist, this can happen if a delegated entity is being despawned");
            return;
        }
        // Priority layer eviction: drop global entry + every user's per-user
        // entry for this entity. Prevents leaks across entity lifetime.
        self.global_priority.on_despawn(world_entity);
        for layer in self.user_priorities.values_mut() {
            layer.on_scope_exit(world_entity);
        }
        // Drop every (*, *, world_entity) tuple from the scope-checks cache.
        // Single linear retain — covers all rooms that previously contained
        // the entity, replacing what would otherwise be one retain per
        // affected room.
        self.scope_checks_cache
            .on_entity_despawned(*world_entity);
        self.cleanup_entity_replication(&global_entity);
        self.global_world_manager
            .remove_entity_record(&global_entity);
        self.global_entity_map.despawn_by_global(&global_entity);
    }

    fn cleanup_entity_replication(&mut self, global_entity: &GlobalEntity) {
        self.despawn_entity_from_all_connections(global_entity);

        // Delete scope
        self.entity_scope_map.remove_entity(global_entity);

        // Delete room cache entry
        if let Some(room_keys) = self.entity_room_map.remove_from_all_rooms(global_entity) {
            for room_key in room_keys {
                if let Some(room) = self.room_store.get_mut(&room_key) {
                    room.remove_entity(global_entity, true);
                }
            }
        }

        // Remove from ECS Record
        self.global_world_manager
            .remove_entity_diff_handlers(global_entity);
    }

    fn despawn_entity_from_all_connections(&mut self, global_entity: &GlobalEntity) {
        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_, connection) in self.user_connections.iter_mut() {
            if !connection
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                continue;
            }
            // remove entity from user connection
            connection.base.world_manager.despawn_entity(global_entity);
        }
    }

    //// Entity Scopes

    /// Remove all entities from a User's scope
    pub(crate) fn user_scope_remove_user(&mut self, user_key: &UserKey) {
        self.entity_scope_map.remove_user(user_key);
    }

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        world_entity: &E,
        is_contained: bool,
    ) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Per [entity-authority-12]: If the authority-holding client loses scope for E,
        // the server MUST release/reset authority for E.
        // Check if user is being removed from scope and is the authority holder
        if !is_contained
            && self
                .global_world_manager
                .user_is_authority_holder(user_key, &global_entity)
        {
            // Release authority - the user is losing scope while holding authority
            let releaser = AuthOwner::Client(*user_key);
            if self
                .global_world_manager
                .client_release_authority(&global_entity, &releaser)
                .is_ok()
            {
                // Notify other clients that authority is now Available
                self.send_reset_authority_messages(&global_entity);
            }
        }

        // Per [entity-publication]: silently ignore explicit include() for Private entities
        // when the user is not the owner — mirrors the guard in user_scope_has_entity().
        if is_contained {
            let is_private = self
                .global_world_manager
                .entity_replication_config(&global_entity)
                .map(|c| matches!(c.publicity, Publicity::Private))
                .unwrap_or(false);
            if is_private {
                let is_owner = match self.global_world_manager.entity_owner(&global_entity) {
                    Some(
                        EntityOwner::Client(owner_key)
                        | EntityOwner::ClientWaiting(owner_key)
                        | EntityOwner::ClientPublic(owner_key),
                    ) => owner_key == *user_key,
                    _ => false,
                };
                if !is_owner {
                    return;
                }
            }
        }

        self.entity_scope_map
            .insert(*user_key, global_entity, is_contained);
        self.scope_change_queue.push_back(ScopeChange::ScopeToggled(
            *user_key,
            global_entity,
            is_contained,
        ));
    }

    pub(crate) fn user_scope_has_entity(&self, user_key: &UserKey, world_entity: &E) -> bool {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        // Check if entity has Private replication config
        let is_private = if let Some(config) = self
            .global_world_manager
            .entity_replication_config(&global_entity)
        {
            matches!(config.publicity, Publicity::Private)
        } else {
            false
        };

        // Owning client is always in-scope for client-owned entities
        let is_owner = if let Some(
            EntityOwner::Client(owner_key)
            | EntityOwner::ClientWaiting(owner_key)
            | EntityOwner::ClientPublic(owner_key),
        ) = self.global_world_manager.entity_owner(&global_entity)
        {
            owner_key == *user_key
        } else {
            false
        };

        // If owner, always in scope
        if is_owner {
            return true;
        }

        // Per [entity-publication]: Private entities MUST NOT be in-scope for non-owners
        if is_private {
            return false;
        }

        // Check explicit include/exclude
        if let Some(in_scope) = self.entity_scope_map.get(user_key, &global_entity) {
            if *in_scope {
                // [entity-scopes-09]: explicit include() cannot bypass the room gate for
                // server-owned non-resource entities that have no rooms at all. Entities
                // in rooms (even rooms the user isn't in) are valid include() targets per
                // [entity-scopes-06]; only completely roomless entities are gated.
                let entity_is_roomless = self
                    .entity_room_map
                    .entity_get_rooms(&global_entity)
                    .is_none();
                if entity_is_roomless {
                    let is_resource = self.resource_registry.is_resource_entity(&global_entity);
                    let server_owned = self
                        .global_world_manager
                        .entity_owner(&global_entity)
                        .map(|o| o.is_server())
                        .unwrap_or(false);
                    if server_owned && !is_resource {
                        return false;
                    }
                }
            }
            return *in_scope;
        }
        // Default: in-scope if user and entity share a room
        let Some(user) = self.user_store.get(user_key) else {
            return false;
        };
        let Some(entity_rooms) = self.entity_room_map.entity_get_rooms(&global_entity) else {
            return false;
        };
        let user_rooms = user.room_keys();
        entity_rooms.intersection(user_rooms).next().is_some()
    }

    //// Components

    /// Adds a Component to an Entity
    pub(crate) fn insert_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
        mut component: R,
    ) {
        if !world.has_entity(world_entity) {
            panic!("attempted to add component to non-existent entity");
        }

        let component_kind = component.kind();

        if world.has_component_of_kind(world_entity, &component_kind) {
            // Entity already has this Component type yet, update Component

            let Some(mut component_mut) = world.component_mut::<R>(world_entity) else {
                panic!("Should never happen because we checked for this above");
            };
            component_mut.mirror(&component);
        } else {
            // Entity does not have this Component type yet, initialize Component
            self.insert_component_worldless(world_entity, &mut component);

            // actually insert component into world
            world.insert_component(world_entity, component);
        }
    }

    /// Registers a component insertion in the replication layer without touching the world (adapter use only).
    pub fn insert_component_worldless(&mut self, world_entity: &E, component: &mut dyn Replicate) {
        let component_kind = component.kind();

        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();

        if self
            .global_world_manager
            .has_component_record(&global_entity, &component_kind)
        {
            warn!(
                "Attempted to add component `{:?}` to entity `{:?}` that already has it, this can happen if a delegated entity's auth is transferred to the Server before the Server Adapter has been able to process the newly inserted Component. Skipping this action.",
                component.name(), global_entity,
            );
            return;
        }

        self.insert_new_component_into_entity_scopes(&global_entity, &component_kind, None);

        // update in world manager
        self.global_world_manager.insert_component_record(
            // &self.component_kinds,
            &global_entity,
            &component_kind,
        );
        self.global_world_manager.insert_component_diff_handler(
            &self.component_kinds,
            &global_entity,
            component,
        );

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

    fn insert_new_component_into_entity_scopes(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        excluding_user_opt: Option<&UserKey>,
    ) {
        let excluding_addr_opt: Option<SocketAddr> = {
            if let Some(user_key) = excluding_user_opt {
                self.user_store.get(user_key).map(|user| user.address())
            } else {
                None
            }
        };
        // add component to connections already tracking entity
        for (addr, connection) in self.user_connections.iter_mut() {
            if let Some(exclude_addr) = excluding_addr_opt {
                if addr == &exclude_addr {
                    continue;
                }
            }

            // insert component into user's connection
            let has_entity = connection
                .base
                .world_manager
                .has_global_entity(global_entity);

            if !has_entity {
                // entity is not in scope for this connection
                continue;
            }
            connection
                .base
                .world_manager
                .insert_component(global_entity, component_kind);
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: ReplicatedComponent, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        world_entity: &E,
    ) -> Option<R> {
        self.remove_component_worldless(world_entity, &ComponentKind::of::<R>());

        // remove from world
        world.remove_component::<R>(world_entity)
    }

    /// Removes a component from the replication layer without touching the world (adapter use only).
    pub fn remove_component_worldless(&mut self, world_entity: &E, component_kind: &ComponentKind) {
        let global_entity = self
            .global_entity_map
            .entity_to_global_entity(world_entity)
            .unwrap();
        self.remove_component_from_all_connections(&global_entity, component_kind);

        // cleanup all other loose ends
        self.global_world_manager
            .remove_component_record(&global_entity, component_kind);
        self.global_world_manager
            .remove_component_diff_handler(&global_entity, component_kind);
    }

    fn remove_component_from_all_connections(
        &mut self,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) {
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for (_, connection) in self.user_connections.iter_mut() {
            if !connection
                .base
                .world_manager
                .has_global_entity(global_entity)
            {
                // entity is not in scope for this connection
                continue;
            }
            // remove component from user connection
            connection
                .base
                .world_manager
                .remove_component(global_entity, component_kind);
        }
    }

    //// Authority

    pub(crate) fn publish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) -> bool {
        if server_origin {
            // send publish message to entity owner
            let entity_owner = self.global_world_manager.entity_owner(global_entity);
            let Some(EntityOwner::Client(user_key)) = entity_owner else {
                panic!(
                    "Entity is not owned by a Client. Cannot publish entity. Owner is: {:?}",
                    entity_owner
                );
            };
            // Send PublishEntity action through EntityActionEvent system
            if let Some(user) = self.user_store.get(&user_key) {
                if let Some(connection) = self.user_connections.get_mut(&user.address()) {
                    connection
                        .base
                        .world_manager
                        .send_publish(HostType::Server, global_entity);
                }
            }
        }

        let result = self.global_world_manager.entity_publish(global_entity);
        if result {
            world.entity_publish(
                &self.component_kinds,
                &self.global_entity_map,
                &self.global_world_manager,
                world_entity,
            );
            // Re-evaluate scope for every user who shares a room with this entity.
            // The EntityEnteredRoom change was already processed when Private (and
            // returned early); now that the entity is Public we must trigger it again.
            let entity_rooms: Vec<RoomKey> = self
                .entity_room_map
                .entity_get_rooms(global_entity)
                .map(|rooms| rooms.iter().copied().collect())
                .unwrap_or_default();
            for room_key in entity_rooms {
                self.scope_change_queue
                    .push_back(ScopeChange::EntityEnteredRoom(*global_entity, room_key));
            }
        }
        result
    }

    pub(crate) fn unpublish_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        server_origin: bool,
    ) {
        // Capture the owner's connection address before state change.
        // entity_unpublish() transitions the owner from ClientPublic → Client,
        // so we read it here while it is still ClientPublic.
        let owner_addr: Option<SocketAddr> = self
            .global_world_manager
            .entity_owner(global_entity)
            .and_then(|o| if let EntityOwner::ClientPublic(k) = o { Some(k) } else { None })
            .and_then(|k| self.user_store.get(&k).map(|u| u.address()));

        if server_origin {
            // Send UnpublishEntity action through EntityActionEvent system
            if let Some(addr) = owner_addr {
                if let Some(connection) = self.user_connections.get_mut(&addr) {
                    connection
                        .base
                        .world_manager
                        .send_unpublish(HostType::Server, global_entity);
                }
            }
        }

        self.global_world_manager.entity_unpublish(global_entity);
        world.entity_unpublish(world_entity);

        // Deregister each component from the diff handler so re-publishing
        // can register them again without the "cannot Register more than once" panic.
        if let Some(kinds) = self.global_world_manager.component_kinds(global_entity) {
            for component_kind in kinds {
                self.global_world_manager
                    .remove_component_diff_handler(global_entity, &component_kind);
            }
        }

        // Despawn from non-owner connections only.  Scope map entries and room
        // membership are preserved so a subsequent publish_entity call restores
        // non-owner visibility via room-based scope (entity-publication-11).
        for (addr, connection) in self.user_connections.iter_mut() {
            if owner_addr == Some(*addr) {
                continue;
            }
            if connection.base.world_manager.has_global_entity(global_entity) {
                connection.base.world_manager.despawn_entity(global_entity);
            }
        }
    }

    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_origin: Option<UserKey>,
    ) {
        // TODO: check that entity is eligible for delegation?

        {
            // For any users that have this entity in scope,
            // Send an `enable_delegation` message

            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (user_key, user) in self.user_store.iter() {
                if let Some(client_key) = &client_origin {
                    if user_key == client_key {
                        // skip sending to origin client, will be handled below
                        continue;
                    }
                }

                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send EnableDelegationEntity action through EntityActionEvent system
                info!(
                    "Sending EnableDelegation command for entity: {:?} for user: {:?}",
                    global_entity,
                    user.address()
                );
                connection.base.world_manager.send_enable_delegation(
                    HostType::Server,
                    client_origin.is_some(),
                    global_entity,
                );
            }
        }

        if let Some(client_key) = client_origin {
            self.enable_delegation_client_owned_entity(
                world,
                global_entity,
                world_entity,
                &client_key,
            );
        } else {
            self.global_world_manager
                .entity_enable_delegation(global_entity);
            world.entity_enable_delegation(
                &self.component_kinds,
                &self.global_entity_map,
                &self.global_world_manager,
                world_entity,
            );
        }
    }

    pub(crate) fn enable_delegation_client_owned_entity<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
        client_key: &UserKey,
    ) {
        let Some(entity_owner) = self.global_world_manager.entity_owner(global_entity) else {
            panic!("entity should have an owner at this point");
        };
        let owner_user_key;
        match entity_owner {
            EntityOwner::Client(user_key) => {
                // The entity was spawned by the client but the Publish packet
                // has not yet arrived (enable-delegation arrived first due to
                // packet reordering). Promote the entity to ClientPublic now so
                // delegation setup can proceed — the Publish packet, when it
                // arrives, will be a no-op since the entity is already public.
                // This is the correct handling for the publish-after-delegation
                // packet-ordering race; it is NOT a shortcut around the protocol.
                owner_user_key = user_key;
                let result = self.global_world_manager.entity_publish(global_entity);
                if !result {
                    warn!(
                        "enable_delegation_client_owned_entity: entity_publish failed for {:?}; \
                         aborting delegation enable (entity may already be public or in an \
                         inconsistent state)",
                        global_entity
                    );
                    return;
                }
                world.entity_publish(
                    &self.component_kinds,
                    &self.global_entity_map,
                    &self.global_world_manager,
                    world_entity,
                );
            }
            EntityOwner::ClientPublic(user_key) => {
                owner_user_key = user_key;
            }
            _owner => {
                panic!(
                    "entity should be owned by a public client at this point. Owner is: {:?}",
                    entity_owner
                );
            }
        }
        let user_key = owner_user_key;
        self.global_world_manager
            .migrate_entity_to_server(global_entity);

        // Initialize the former-owner's scope entry to "in scope" only if it
        // wasn't already set. The check at the end of this method consults
        // `entity_scope_map` directly to decide whether to grant initial
        // authority to the former owner — overwriting an explicit exclude
        // would silently grant authority to a user who had been put
        // out-of-scope by the application (contract
        // [entity-delegation-09]: "migration yields no holder if owner is
        // out of scope at migration time").
        if self
            .entity_scope_map
            .get(&user_key, global_entity)
            .is_none()
        {
            self.entity_scope_map.insert(user_key, *global_entity, true);
        }

        // Migrate Entity from Remote -> Host connection
        let Some(user) = self.user_store.get(&user_key) else {
            panic!("user should exist");
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            panic!("connection does not exist")
        };

        // Step 0: Capture old RemoteEntity BEFORE migration (will be needed for MigrateResponse)
        let old_remote_entity = match connection
            .base
            .world_manager
            .entity_converter()
            .global_entity_to_remote_entity(global_entity)
        {
            Ok(entity) => entity,
            Err(_) => {
                panic!(
                    "Entity must exist as RemoteEntity before delegation: {:?}",
                    global_entity
                );
            }
        };

        // Step 1: Migrate entity from RemoteEntity to HostEntity
        // This creates the HostEntity in HostEngine so it can receive commands
        let new_host_entity = match connection
            .base
            .world_manager
            .migrate_entity_remote_to_host(global_entity)
        {
            Ok(entity) => entity,
            Err(e) => {
                panic!("Failed to migrate entity during delegation: {}", e);
            }
        };

        // Step 2: Force the server's HostEntityChannel into Delegated state locally
        // This allows MigrateResponse to be sent (requires Delegated state)
        // We do NOT send EnableDelegation back to the client - they already sent it!
        connection
            .base
            .world_manager
            .host_local_enable_delegation(&new_host_entity);

        // Step 3: Send MigrateResponse to client
        // This will be the FIRST message in the new HostEntityChannel sequence (subcommand_id=0)
        connection.base.world_manager.host_send_migrate_response(
            global_entity,
            &old_remote_entity,
            &new_host_entity,
        );

        self.global_world_manager
            .entity_enable_delegation(global_entity);
        world.entity_enable_delegation(
            &self.component_kinds,
            &self.global_entity_map,
            &self.global_world_manager,
            world_entity,
        );

        // Per contracts [entity-delegation-06]/[07]/[08]/[09]: the
        // previous owner gets initial Granted authority *iff* it's
        // still in-scope for the entity at migration time. If the
        // owner is out-of-scope, no holder is assigned and every
        // in-scope client observes Available (the default emitted by
        // EnableDelegation). We use `entity_scope_map` directly
        // because `user_scope_has_entity` takes a world_entity (E),
        // not a global_entity, and we only have the global here.
        let owner_in_scope = self
            .entity_scope_map
            .get(client_key, global_entity)
            .copied()
            .unwrap_or(false);

        if owner_in_scope {
            let requester = AuthOwner::from_user_key(Some(client_key));
            let result = self
                .global_world_manager
                .client_request_authority(global_entity, &requester);
            if result.is_err() {
                panic!("failed to grant authority of client-owned delegated entity to creating user");
            }

            // Fan out SetAuthority to every in-scope user so the holder
            // observes Granted and everyone else observes Denied.
            // Without this, the per-client auth status stays at the
            // EnableDelegation default (Available) and contracts
            // [entity-delegation-06]/[entity-delegation-07] (migration
            // assigns initial authority to the previous owner) silently
            // fail. Snapshot first so we can re-borrow user_connections
            // mutably inside the loop.
            let user_snapshot: Vec<(UserKey, std::net::SocketAddr)> = self
                .user_store
                .iter()
                .map(|(k, u)| (*k, u.address()))
                .collect();
            for (user_key, addr) in user_snapshot {
                let Some(connection) = self.user_connections.get_mut(&addr) else {
                    continue;
                };
                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    continue;
                }
                let new_status = if user_key == *client_key {
                    EntityAuthStatus::Granted
                } else {
                    EntityAuthStatus::Denied
                };
                connection
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, new_status);
            }
        }
        // else: owner is out-of-scope — leave AuthOwner::None and don't
        // emit any SetAuthority. Every in-scope client already sees
        // Available from the EnableDelegation default.
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        global_entity: &GlobalEntity,
        world_entity: &E,
    ) {
        // TODO: check that entity is eligible for delegation?

        // for any users that have this entity in scope, send an `disable_delegation` message
        {
            // TODO: we can make this more efficient in the future by caching which Entities
            // are in each User's scope
            for (_user_key, user) in self.user_store.iter() {
                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                if !connection
                    .base
                    .world_manager
                    .has_global_entity(global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // Send DisableDelegationEntity action through EntityActionEvent system
                connection
                    .base
                    .world_manager
                    .send_disable_delegation(global_entity);
            }
        }

        self.global_world_manager
            .entity_disable_delegation(global_entity);
        world.entity_disable_delegation(world_entity);
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        self.user_store.address(user_key)
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub(crate) fn user_room_keys(&'_ self, user_key: &UserKey) -> Option<Iter<'_, RoomKey>> {
        self.user_store.room_keys_iter(user_key)
    }

    /// Get an count of how many Rooms the given User is inside
    pub(crate) fn user_rooms_count(&self, user_key: &UserKey) -> Option<usize> {
        self.user_store.rooms_count(user_key)
    }

    pub(crate) fn user_disconnect<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        reason: DisconnectReason,
        world: &mut W,
    ) {
        if self.client_authoritative_entities {
            self.despawn_all_remote_entities(user_key, world);
            if let Some(all_owned_entities) =
                self.global_world_manager.user_all_owned_entities(user_key)
            {
                let copied_entities = all_owned_entities.clone();
                for global_entity in copied_entities {
                    // Only release authority if entity still exists (may have been despawned already)
                    if let Ok(world_entity) = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                    {
                        let _ = self.entity_release_authority(Some(user_key), &world_entity);
                    }
                }
            }
        }
        let user = self.user_delete(user_key);
        self.incoming_world_events
            .push_disconnection(user_key, user.address(), reason);
    }

    pub(crate) fn user_queue_disconnect(&mut self, user_key: &UserKey, reason: DisconnectReason) {
        let Some(user) = self.user_store.get(user_key) else {
            // User already disconnected, this is fine (disconnect packets may arrive multiple times)
            return;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            // Connection already gone, user is being/has been disconnected
            return;
        };

        // If already marked for disconnect, don't queue again (idempotent)
        if connection.manual_disconnect {
            return;
        }

        connection.manual_disconnect = true;
        // Add to outstanding_disconnects immediately so it gets processed in the next process_all_packets call
        self.outstanding_disconnects.push((*user_key, reason));
    }

    pub(crate) fn user_delete(&mut self, user_key: &UserKey) -> WorldUser {
        let Some(user) = self.user_store.remove(user_key) else {
            panic!("Attempting to delete non-existent user!");
        };

        let user_addr = user.address();

        info!("deleting authenticated user for {}", user.address());
        self.user_connections.remove(&user_addr);

        // Drop this user's entire per-user priority layer so entries never
        // leak across user sessions.
        self.user_priorities.remove(user_key);

        self.entity_scope_map.remove_user(user_key);

        // Clean up all user data
        for room_key in user.room_keys() {
            self.room_store
                .get_mut(room_key)
                .unwrap()
                .unsubscribe_user(user_key);
            // Mirror the room→user removal into the scope-checks cache —
            // this path bypasses `room_remove_user`.
            self.scope_checks_cache
                .on_user_removed_from_room(*room_key, *user_key);
        }

        // remove from bandwidth monitor
        if self.io.bandwidth_monitor_enabled() {
            self.io.deregister_client(&user.address());
        }

        self.global_request_manager.purge_user(user_key);
        self.global_response_manager.purge_user(user_key);

        user
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn despawn_all_remote_entities<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        let Some(user) = self.user_store.get(user_key) else {
            panic!("Attempting to despawn entities for a nonexistent user");
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            panic!("Attempting to despawn entities on a nonexistent connection");
        };

        let remote_global_entities = connection.base.world_manager.remote_entities();
        let entity_events = SharedGlobalWorldManager::despawn_all_entities(
            world,
            &self.global_entity_map,
            &self.global_world_manager,
            remote_global_entities,
        );
        self.process_entity_events(world, user_key, entity_events);
    }

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        let Self { room_store, user_store, entity_room_map, scope_checks_cache, .. } = self;
        room_store.destroy(room_key, user_store, entity_room_map, scope_checks_cache)
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        self.room_store.has_user(room_key, user_key)
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        let Self { room_store, user_store, global_entity_map, scope_checks_cache, scope_change_queue, .. } = self;
        let change = room_store.add_user(room_key, user_key, user_store, global_entity_map, scope_checks_cache);
        scope_change_queue.push_back(change);
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        #[cfg(feature = "e2e_debug")]
        {
            SERVER_ROOM_MOVE_CALLED.fetch_add(1, Ordering::Relaxed);
        }
        let Self { room_store, user_store, scope_checks_cache, scope_change_queue, .. } = self;
        let change = room_store.remove_user(room_key, user_key, user_store, scope_checks_cache);
        scope_change_queue.push_back(change);
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: &RoomKey) -> usize {
        self.room_store.users_count(room_key)
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the Room
    pub(crate) fn room_user_keys(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        self.room_store.user_keys_iter(room_key)
    }

    pub(crate) fn room_entities(&self, room_key: &RoomKey) -> impl Iterator<Item = &GlobalEntity> {
        self.room_store.entities_iter(room_key)
    }

    /// Sends a message to all connected users in a given Room using a given channel
    pub(crate) fn room_broadcast_message(
        &mut self,
        channel_kind: &ChannelKind,
        room_key: &RoomKey,
        message_box: Box<dyn Message>,
    ) {
        // Wrap once in Arc so per-user clones are refcount increments, not heap allocs.
        let container = MessageContainer::new(message_box);
        let user_keys: Vec<UserKey> = self.room_store.user_keys_iter(room_key).cloned().collect();
        for user_key in &user_keys {
            let _ = self.send_message_inner(user_key, channel_kind, container.clone());
        }
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity: &GlobalEntity) -> bool {
        self.room_store.has_entity(room_key, entity)
    }

    /// Add an Entity to a Room associated with the given RoomKey.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let Self { room_store, global_entity_map, entity_room_map, scope_checks_cache, scope_change_queue, .. } = self;
        if let Some(change) = room_store.add_entity(room_key, world_entity, global_entity_map, entity_room_map, scope_checks_cache) {
            scope_change_queue.push_back(change);
        }
    }

    /// Remove an Entity from a Room, associated with the given RoomKey
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, world_entity: &E) {
        let Self { room_store, global_entity_map, entity_room_map, scope_checks_cache, .. } = self;
        room_store.remove_entity(room_key, world_entity, global_entity_map, entity_room_map, scope_checks_cache);
    }


    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        self.room_store.entities_count(room_key)
    }

    // Private methods

    fn read_data_packet(
        &mut self,
        address: &SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        if header.packet_type != PacketType::Data {
            panic!("Server Error: received non-data packet in data packet handler");
        }

        let Some(connection) = self.user_connections.get_mut(address) else {
            return Ok(());
        };

        #[cfg(feature = "e2e_debug")]
        {
            SERVER_RX_FRAMES.fetch_add(1, Ordering::Relaxed);
        }

        // Process incoming header
        connection.process_incoming_header(header);

        // read client tick
        let client_tick = Tick::de(reader)?;

        let server_tick = self.time_manager.current_tick();

        // process data
        connection.read_packet(
            &self.channel_kinds,
            &self.message_kinds,
            &self.component_kinds,
            self.client_authoritative_entities,
            server_tick,
            client_tick,
            reader,
        )?;

        // Mark that we should send an ACK-only packet
        connection.base.mark_should_send_empty_ack();

        Ok(())
    }

    fn process_disconnects<W: WorldMutType<E>>(&mut self, world: &mut W) {
        let user_disconnects = std::mem::take(&mut self.outstanding_disconnects);
        for (user_key, reason) in user_disconnects {
            self.user_disconnect(&user_key, reason, world);
        }
    }

    fn process_packets<W: WorldMutType<E>>(
        &mut self,
        address: &SocketAddr,
        world: &mut W,
        now: &Instant,
    ) {
        // Packets requiring established connection
        let (user_key, entity_events) = {
            let Some(connection) = self.user_connections.get_mut(address) else {
                return;
            };
            (
                connection.user_key,
                connection.process_packets(
                    &self.message_kinds,
                    &self.component_kinds,
                    self.client_authoritative_entities,
                    now,
                    &mut self.global_entity_map,
                    &mut self.global_world_manager,
                    &mut self.global_request_manager,
                    &mut self.global_response_manager,
                    world,
                    &mut self.incoming_world_events,
                ),
            )
        };
        self.process_entity_events(world, &user_key, entity_events);
    }

    fn process_entity_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        user_key: &UserKey,
        response_events: Vec<EntityEvent>,
    ) {
        let mut deferred_events = Vec::new();
        for response_event in response_events {
            match response_event {
                EntityEvent::Spawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_spawn(user_key, &world_entity);
                    self.global_world_manager
                        .insert_entity_record(&global_entity, EntityOwner::Client(*user_key));
                    let user = self.user_store.get(user_key).unwrap();
                    let connection = self.user_connections.get_mut(&user.address()).unwrap();
                    connection
                        .base
                        .world_manager
                        .remote_spawn_entity(&global_entity); // TODO: migrate to localworldmanager
                    #[cfg(feature = "e2e_debug")]
                    {
                        SERVER_SPAWN_APPLIED.fetch_add(1, Ordering::Relaxed);
                    }
                }
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    deferred_events.push(EntityEvent::Despawn(global_entity));
                }
                EntityEvent::InsertComponent(global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_insert(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                    self.global_world_manager.insert_component_record(
                        // &self.component_kinds,
                        &global_entity,
                        &component_kind,
                    );
                    let is_public_and_client_owned = self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity);
                    let is_delegated = self
                        .global_world_manager
                        .entity_is_delegated(&global_entity);

                    if is_public_and_client_owned || is_delegated {
                        world.component_publish(
                            &self.component_kinds,
                            &self.global_entity_map,
                            &self.global_world_manager,
                            &world_entity,
                            &component_kind,
                        );

                        if is_delegated {
                            world.component_enable_delegation(
                                &self.component_kinds,
                                &self.global_entity_map,
                                &self.global_world_manager,
                                &world_entity,
                                &component_kind,
                            );
                        }

                        self.insert_new_component_into_entity_scopes(
                            &global_entity,
                            &component_kind,
                            Some(user_key),
                        );
                    }
                }
                EntityEvent::RemoveComponent(global_entity, component) => {
                    let component_kind = component.kind();
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_remove(user_key, &world_entity, component);
                    if self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity)
                        || self
                            .global_world_manager
                            .entity_is_delegated(&global_entity)
                    {
                        self.remove_component_worldless(&world_entity, &component_kind);
                    } else {
                        self.global_world_manager
                            .remove_component_record(&global_entity, &component_kind);
                    }
                }
                EntityEvent::UpdateComponent(_tick, global_entity, component_kind) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events.push_update(
                        user_key,
                        &world_entity,
                        &component_kind,
                    );
                }
                _ => {
                    deferred_events.push(response_event);
                }
            }
        }

        let mut extra_deferred_events = Vec::new();
        // The reason for deferring these events is that they depend on the operations to the world above
        for response_event in deferred_events {
            match response_event {
                EntityEvent::Publish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.publish_entity(world, &global_entity, &world_entity, false);
                    self.incoming_world_events
                        .push_publish(user_key, &world_entity);

                    // NOTE: Client-owned entities do NOT get auto-granted authority.
                    // Authority/SetAuthority only applies to delegated (server-owned) entities.
                }
                EntityEvent::Unpublish(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.unpublish_entity(world, &global_entity, &world_entity, false);
                    self.incoming_world_events
                        .push_unpublish(user_key, &world_entity);
                }
                EntityEvent::EnableDelegation(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.entity_enable_delegation(
                        world,
                        &global_entity,
                        &world_entity,
                        Some(*user_key),
                    );
                    self.incoming_world_events
                        .push_delegate(user_key, &world_entity);
                }
                EntityEvent::EnableDelegationResponse(global_entity) => {
                    self.entity_enable_delegation_response(user_key, &global_entity);
                }
                EntityEvent::DisableDelegation(_) => {
                    panic!("Clients should not be able to disable entity delegation.");
                }
                EntityEvent::RequestAuthority(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if self.entity_handle_client_request_authority(user_key, &world_entity).is_err() {
                        self.incoming_world_events.push_auth_denied(user_key, &world_entity);
                    }
                }
                EntityEvent::ReleaseAuthority(global_entity) => {
                    // info!("received release auth entity message!");
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    if self
                        .entity_release_authority(Some(user_key), &world_entity)
                        .is_ok()
                    {
                        self.incoming_world_events.push_auth_reset(&world_entity);
                    }
                }
                EntityEvent::SetAuthority(_, _) => {
                    panic!("Clients should not be able to update entity authority.");
                }
                EntityEvent::MigrateResponse(_, _) => {
                    panic!("Clients should not be able to send this message");
                }
                _ => {
                    extra_deferred_events.push(response_event);
                }
            }
        }

        for response_event in extra_deferred_events {
            match response_event {
                EntityEvent::Despawn(global_entity) => {
                    let world_entity = self
                        .global_entity_map
                        .global_entity_to_entity(&global_entity)
                        .unwrap();
                    self.incoming_world_events
                        .push_despawn(user_key, &world_entity);
                    let owner = self.global_world_manager.entity_owner(&global_entity);
                    if self
                        .global_world_manager
                        .entity_is_public_and_client_owned(&global_entity)
                        || (self
                            .global_world_manager
                            .entity_is_delegated(&global_entity)
                            && matches!(
                                owner,
                                Some(
                                    EntityOwner::Client(_)
                                        | EntityOwner::ClientPublic(_)
                                        | EntityOwner::ClientWaiting(_)
                                )
                            ))
                    {
                        // Client-created delegated entity: remove from this connection's remote
                        // world manager, then tear down the server-side entity record entirely.
                        let user = self.user_store.get(user_key).unwrap();
                        let connection = self.user_connections.get_mut(&user.address()).unwrap();
                        connection
                            .base
                            .world_manager
                            .remote_despawn_entity(&global_entity);

                        self.despawn_entity_worldless(&world_entity);
                    } else if self
                        .global_world_manager
                        .entity_is_delegated(&global_entity)
                    {
                        // Server-created delegated entity despawned by the authority-holding client.
                        // The entity lives in the server's host world manager, not in any remote
                        // world manager, so skip remote_despawn_entity and go straight to full teardown.
                        self.despawn_entity_worldless(&world_entity);
                    } else {
                        self.global_world_manager
                            .remove_entity_record(&global_entity);
                        self.global_entity_map.despawn_by_global(&global_entity);
                    }
                }
                _ => {
                    panic!("shouldn't happen");
                }
            }
        }
    }

    fn handle_pings(&mut self) {
        // pings
        if self.ping_timer.ringing() {
            self.ping_timer.reset();

            for (user_address, connection) in &mut self.user_connections.iter_mut() {
                // send pings
                if connection.ping_manager.should_send_ping() {
                    let mut writer = BitWriter::new();

                    // write header
                    let _header = connection.base.write_header(PacketType::Ping, &mut writer);

                    // write server tick
                    self.time_manager.current_tick().ser(&mut writer);

                    // write server tick instant
                    self.time_manager.current_tick_instant().ser(&mut writer);

                    // write body
                    connection
                        .ping_manager
                        .write_ping(&mut writer, &self.time_manager);

                    // send packet
                    if self
                        .io
                        .send_packet(user_address, writer.to_packet())
                        .is_err()
                    {
                        // Ping send failure is not fatal: the connection timeout
                        // will detect a persistently dead link via missed pongs.
                        warn!("Server Error: Cannot send ping packet to {}", user_address);
                    }
                    connection.base.mark_sent();
                }
            }
        }
    }

    fn handle_heartbeats(&mut self) {
        // heartbeats
        if self.heartbeat_timer.ringing() {
            self.heartbeat_timer.reset();

            for (user_address, connection) in &mut self.user_connections.iter_mut() {
                // user heartbeats
                if connection.base.should_send_heartbeat() {
                    Self::send_heartbeat_packet(
                        user_address,
                        connection,
                        &self.time_manager,
                        &mut self.io,
                    );
                }
            }
        }
    }

    fn send_heartbeat_packet(
        user_address: &SocketAddr,
        connection: &mut Connection,
        time_manager: &TimeManager,
        io: &mut Io,
    ) {
        // Don't try to refactor this to self.internal_send, doesn't seem to
        // work cause of iter_mut()
        let mut writer = BitWriter::new();

        // write header
        let _header = connection
            .base
            .write_header(PacketType::Heartbeat, &mut writer);

        // write server tick
        time_manager.current_tick().ser(&mut writer);

        // write server tick instant
        time_manager.current_tick_instant().ser(&mut writer);

        // send packet
        if io.send_packet(user_address, writer.to_packet()).is_err() {
            // Heartbeat send failure is not fatal: the connection timeout
            // will detect a persistently dead link when heartbeats stop arriving.
            warn!(
                "Server Error: Cannot send heartbeat packet to {}",
                user_address
            );
        }
        connection.base.mark_sent();
    }

    fn handle_empty_acks(&mut self) {
        // empty acks

        for (user_address, connection) in &mut self.user_connections.iter_mut() {
            if connection.base.should_send_empty_ack() {
                Self::send_heartbeat_packet(
                    user_address,
                    connection,
                    &self.time_manager,
                    &mut self.io,
                );
            }
        }
    }

    // Entity Scopes

    fn update_entity_scopes<W: WorldRefType<E>>(&mut self, world: &W) {
        // Loop 1 (both paths): drain per-room entity-removal queues.
        // This handles entities removed from a room via room_remove_entity.
        for (_, room) in self.room_store.iter_mut() {
            while let Some((removed_user, removed_global_entity)) = room.pop_entity_removal_queue()
            {
                let Some(user) = self.user_store.get(&removed_user) else {
                    continue;
                };
                let Some(connection) = self.user_connections.get_mut(&user.address()) else {
                    continue;
                };

                // evaluate whether the Entity really needs to be despawned!
                // what if the Entity shares another Room with this User? It shouldn't be despawned!
                if let Some(entity_rooms) = self
                    .entity_room_map
                    .entity_get_rooms(&removed_global_entity)
                {
                    let user_rooms = user.room_keys();
                    let has_room_in_common = entity_rooms.intersection(user_rooms).next().is_some();
                    if has_room_in_common {
                        continue;
                    }
                }

                // check if host has entity, because it may have been removed from room before despawning, and we don't want to double despawn
                if !connection
                    .base
                    .world_manager
                    .has_global_entity(&removed_global_entity)
                {
                    // entity is not in scope for this connection
                    continue;
                }

                // remove entity from user connection
                connection
                    .base
                    .world_manager
                    .despawn_entity(&removed_global_entity);
                #[cfg(feature = "e2e_debug")]
                {
                    SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Loop 2: process queued scope changes.
        self.drain_scope_change_queue(world);
    }

    fn drain_scope_change_queue<W: WorldRefType<E>>(&mut self, world: &W) {
        // Snapshot the queue so we can re-borrow self mutably for apply_scope_for_user.
        let changes: Vec<ScopeChange> = self.scope_change_queue.drain(..).collect();
        for change in changes {
            match change {
                ScopeChange::UserEnteredRoom(user_key, room_key) => {
                    let entity_list: Vec<GlobalEntity> = self
                        .room_store
                        .get(&room_key)
                        .map(|r| r.entities().copied().collect())
                        .unwrap_or_default();
                    for global_entity in &entity_list {
                        self.apply_scope_for_user(world, &user_key, global_entity);
                    }
                }
                ScopeChange::UserLeftRoom(user_key, room_key) => {
                    let entity_list: Vec<GlobalEntity> = self
                        .room_store
                        .get(&room_key)
                        .map(|r| r.entities().copied().collect())
                        .unwrap_or_default();
                    let Some(user) = self.user_store.get(&user_key) else {
                        continue;
                    };
                    let user_rooms = user.room_keys().clone();
                    let Some(connection) =
                        self.user_connections.get_mut(&user.address().clone())
                    else {
                        continue;
                    };
                    for global_entity in &entity_list {
                        // Only despawn if the user has no other room in common with the entity.
                        if let Some(entity_rooms) =
                            self.entity_room_map.entity_get_rooms(global_entity)
                        {
                            if entity_rooms.iter().any(|rk| user_rooms.contains(rk)) {
                                continue;
                            }
                        }
                        if !connection.base.world_manager.has_global_entity(global_entity) {
                            continue;
                        }
                        let scope_exit = self
                            .global_world_manager
                            .entity_replication_config(global_entity)
                            .map(|c| c.scope_exit)
                            .unwrap_or(ScopeExit::Despawn);
                        match scope_exit {
                            ScopeExit::Persist => {
                                connection.base.world_manager.pause_entity(global_entity);
                            }
                            ScopeExit::Despawn => {
                                connection.base.world_manager.despawn_entity(global_entity);
                            }
                        }
                        #[cfg(feature = "e2e_debug")]
                        {
                            SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                ScopeChange::EntityEnteredRoom(global_entity, room_key) => {
                    let user_keys: Vec<UserKey> = self
                        .room_store
                        .get(&room_key)
                        .map(|r| r.user_keys().copied().collect())
                        .unwrap_or_default();
                    for user_key in &user_keys {
                        self.apply_scope_for_user(world, user_key, &global_entity);
                    }
                }
                ScopeChange::ScopeToggled(user_key, global_entity, _is_included) => {
                    self.apply_scope_for_user(world, &user_key, &global_entity);
                }
            }
        }
    }

    /// Evaluate scope for one (user, entity) pair and apply any spawn/despawn/pause/resume.
    fn apply_scope_for_user<W: WorldRefType<E>>(
        &mut self,
        world: &W,
        user_key: &UserKey,
        global_entity: &GlobalEntity,
    ) {
        let Some(user) = self.user_store.get(user_key) else {
            return;
        };
        let Some(connection) = self.user_connections.get_mut(&user.address()) else {
            return;
        };
        let Some(world_entity) = self
            .global_entity_map
            .global_entity_to_entity(global_entity)
            .ok()
        else {
            return;
        };
        if !world.has_entity(&world_entity) {
            // Entity not yet spawned in Bevy (deferred commands still pending).
            // Re-queue so we retry next frame instead of permanently losing the scope change.
            self.scope_change_queue
                .push_back(ScopeChange::ScopeToggled(*user_key, *global_entity, true));
            return;
        }
        if self
            .global_world_manager
            .entity_is_public_and_owned_by_user(user_key, global_entity)
        {
            // entity is owned by client but public — don't replicate via this path
            return;
        }
        // Per [entity-publication]: Private (Client/ClientWaiting) entities must
        // never be replicated via this path.
        if matches!(
            self.global_world_manager.entity_owner(global_entity),
            Some(EntityOwner::Client(_)) | Some(EntityOwner::ClientWaiting(_))
        ) {
            return;
        }

        let currently_in_scope = connection
            .base
            .world_manager
            .has_global_entity(global_entity);

        // Decide scope membership. Per contract [entity-scopes-06] /
        // [entity-scopes-12]: an explicit user-scope override wins
        // over the room-default rule. Three cases:
        //   - explicit override = Some(true)  → in scope (even if no
        //     room overlap; "include overrides room absence")
        //   - explicit override = Some(false) → out of scope (even with
        //     room overlap; "exclude hides despite shared room")
        //   - explicit override = None        → use the room default
        //     (in scope iff user and entity share a room)
        // Replicated Resources (D14 / §4.3 of RESOURCES_PLAN) bypass
        // the room rule entirely and are unconditionally in-scope for
        // every connected user, but the explicit-exclude override still
        // applies defensively.
        let in_common_room = if let Some(entity_rooms) =
            self.entity_room_map.entity_get_rooms(global_entity)
        {
            entity_rooms.intersection(user.room_keys()).next().is_some()
        } else {
            false
        };
        let explicit = self
            .entity_scope_map
            .get(user_key, global_entity)
            .copied();
        let is_resource = self.resource_registry.is_resource_entity(global_entity);
        // [entity-scopes-09]: explicit include() MUST NOT bypass the room gate for
        // server-owned entities that have no rooms at all. If the entity has rooms
        // (even rooms the user isn't in), include() is a valid cross-room override
        // per [entity-scopes-06]. Resources and client-owned entities are exempt.
        let entity_is_roomless = self
            .entity_room_map
            .entity_get_rooms(global_entity)
            .is_none();
        let server_owned_roomless_non_resource = self
            .global_world_manager
            .entity_owner(global_entity)
            .map(|o| o.is_server())
            .unwrap_or(false)
            && !is_resource
            && entity_is_roomless;
        let should_be_in_scope = match explicit {
            Some(true) if server_owned_roomless_non_resource => false,
            Some(in_scope) => in_scope,
            None => is_resource || in_common_room,
        };
        if should_be_in_scope {
            if currently_in_scope {
                // Entity already present — resume if paused (ScopeExit::Persist re-entry)
                if connection.base.world_manager.is_entity_paused(global_entity) {
                    connection.base.world_manager.resume_entity(global_entity);
                }
                return;
            }
            let component_kinds = self
                .global_world_manager
                .component_kinds(global_entity)
                .unwrap();
            connection
                .base
                .world_manager
                .host_init_entity(global_entity, component_kinds, &self.component_kinds, self.global_world_manager.entity_is_static(global_entity));
            #[cfg(feature = "e2e_debug")]
            {
                SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            }

            if !self.global_world_manager.entity_is_delegated(global_entity) {
                return;
            }
            connection.base.world_manager.send_enable_delegation(
                HostType::Server,
                false,
                global_entity,
            );
            // Re-entering scope on a delegated entity that already has a
            // holder must surface the current holder's state to the
            // freshly-included user — otherwise the EnableDelegation
            // default of Available silently overrides the real Denied
            // status. Per contract [entity-delegation-15] / scope-re-entry:
            // "re-entering scope yields current authority status".
            if self.global_world_manager.entity_has_holder(global_entity) {
                let new_status = if self
                    .global_world_manager
                    .user_is_authority_holder(user_key, global_entity)
                {
                    EntityAuthStatus::Granted
                } else {
                    EntityAuthStatus::Denied
                };
                connection
                    .base
                    .world_manager
                    .host_send_set_auth(global_entity, new_status);
            }
        } else if currently_in_scope {
            // Entity leaving scope — check ScopeExit policy
            let scope_exit = self
                .global_world_manager
                .entity_replication_config(global_entity)
                .map(|c| c.scope_exit)
                .unwrap_or(ScopeExit::Despawn);
            match scope_exit {
                ScopeExit::Persist => {
                    connection.base.world_manager.pause_entity(global_entity);
                }
                ScopeExit::Despawn => {
                    connection.base.world_manager.despawn_entity(global_entity);
                }
            }
            #[cfg(feature = "e2e_debug")]
            {
                SERVER_SCOPE_DIFF_ENQUEUED.fetch_add(1, Ordering::Relaxed);
            }
            // Priority layer eviction: this user's per-user priority entry for
            // this entity is scoped to in-scope lifetime. Drop it regardless
            // of scope-exit policy — a Persist pause still means no outbound
            // traffic for this (user, entity) pair until re-scoped.
            if let Some(layer) = self.user_priorities.get_mut(user_key) {
                layer.on_scope_exit(&world_entity);
            }
        }
    }

    fn handle_disconnects(&mut self) {
        if self.timeout_timer.ringing() {
            self.timeout_timer.reset();

            // Only queue timeout-based disconnects here; manual disconnects are already
            // queued by user_queue_disconnect() when they are initiated.
            let mut user_disconnects: Vec<UserKey> = Vec::new();
            for (_, connection) in self.user_connections.iter() {
                if connection.should_drop() && !connection.manual_disconnect {
                    user_disconnects.push(connection.user_key);
                }
            }
            for user_key in user_disconnects {
                self.outstanding_disconnects.push((user_key, DisconnectReason::TimedOut));
            }
        }
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> EntityAndGlobalEntityConverter<E> for WorldServer<E> {
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

cfg_if! {
    if #[cfg(feature = "test_utils")] {
        impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
            #[doc(hidden)]
            pub fn diff_handler_global_count(&self) -> usize {
                self.global_world_manager.global_diff_handler_count()
            }

            #[doc(hidden)]
            pub fn diff_handler_global_count_by_kind(
                &self,
            ) -> HashMap<naia_shared::ComponentKind, usize> {
                self.global_world_manager.global_diff_handler_count_by_kind()
            }

            #[doc(hidden)]
            pub fn diff_handler_user_counts(&self) -> HashMap<UserKey, usize> {
                self.user_connections
                    .values()
                    .map(|conn| (conn.user_key, conn.diff_handler_receiver_count()))
                    .collect()
            }

            #[doc(hidden)]
            pub fn scope_change_queue_len(&self) -> usize {
                self.scope_change_queue.len()
            }

            #[doc(hidden)]
            pub fn total_dirty_update_count(&self) -> usize {
                self.user_connections
                    .values()
                    .map(|conn| conn.base.world_manager.dirty_update_count())
                    .sum()
            }
        }
    }
}

cfg_if! {
    if #[cfg(feature = "interior_visibility")] {

        use naia_shared::{LocalEntity, OwnedLocalEntity};

        impl<E: Copy + Eq + Hash + Send + Sync> WorldServer<E> {
            /// Returns all LocalEntity IDs for entities replicated to the given user.
            ///
            /// Returns the set of LocalEntity IDs that currently exist for that user
            /// (i.e., all entities replicated to that user).
            /// The ordering doesn't matter.
            ///
            /// # Panics
            ///
            /// Panics if the user does not exist.
            pub fn local_entities(&self, user_key: &UserKey) -> Vec<LocalEntity> {
                let user = self.user_store.get(user_key).expect("User does not exist");
                let connection = self
                    .user_connections
                    .get(&user.address())
                    .expect("User connection does not exist");

                connection.base.world_manager.local_entities()
            }

            /// Retrieves an EntityRef that exposes read-only operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity<W: WorldRefType<E>>(
                &self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityRef<'_, E, W>> {
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity(world, &world_entity))
            }

            /// Retrieves an EntityMut that exposes read and write operations for the Entity
            /// identified by the given LocalEntity for the specified user.
            ///
            /// Returns `None` if:
            /// - The user does not exist
            /// - The LocalEntity doesn't exist for that user
            /// - The entity does not exist in the world
            pub fn local_entity_mut<W: WorldMutType<E>>(
                &mut self,
                world: W,
                user_key: &UserKey,
                local_entity: &LocalEntity,
            ) -> Option<EntityMut<'_, E, W>> {
                let world_entity = self.local_to_world_entity(user_key, local_entity)?;
                if !world.has_entity(&world_entity) {
                    return None;
                }
                Some(self.entity_mut(world, &world_entity))
            }

            pub(crate) fn local_to_world_entity(
                &self,
                user_key: &UserKey,
                local_entity: &LocalEntity
            ) -> Option<E> {
                let user = self.user_store.get(user_key)?;
                let connection = self.user_connections.get(&user.address())?;
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
                user_key: &UserKey,
                world_entity: &E,
            ) -> Option<LocalEntity> {
                let global_entity = self.global_entity_map.entity_to_global_entity(world_entity).ok()?;

                let user = self.user_store.get(user_key)?;
                let connection = self.user_connections.get(&user.address())?;
                let converter = connection.base.world_manager.entity_converter();
                let owned_entity = converter.global_entity_to_owned_entity(&global_entity).ok()?;

                Some(LocalEntity::from(owned_entity))
            }
        }
    }
}
