use std::{
    collections::{hash_set::Iter, HashMap, HashSet},
    hash::Hash,
    net::SocketAddr,
    panic,
    time::Duration,
};

use log::{info, warn};

#[cfg(feature = "bevy_support")]
use bevy_ecs::prelude::Resource;

use naia_shared::{
    BigMap, BitReader, BitWriter, Channel, ChannelKind, ComponentKind,
    EntityAndGlobalEntityConverter, EntityConverterMut, EntityDoesNotExistError,
    EntityResponseEvent, GlobalEntity, Instant, Message, MessageContainer, PacketType, Protocol,
    Replicate, Serde, SerdeErr, SocketConfig, StandardHeader, Tick, Timer, WorldMutType,
    WorldRefType,
};

use crate::{
    connection::{
        connection::Connection,
        handshake_manager::{HandshakeManager, HandshakeResult},
        io::Io,
        tick_buffer_messages::TickBufferMessages,
    },
    time_manager::TimeManager,
    transport::Socket,
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        entity_scope_map::EntityScopeMap, global_world_manager::GlobalWorldManager,
    },
};

use super::{
    error::NaiaServerError,
    events::Events,
    room::{Room, RoomKey, RoomMut, RoomRef},
    server_config::ServerConfig,
    user::{User, UserKey, UserMut, UserRef},
    user_scope::UserScopeMut,
};

/// A server that uses either UDP or WebRTC communication to send/receive
/// messages to/from connected clients, and syncs registered entities to
/// clients to whom they are in-scope
#[cfg_attr(feature = "bevy_support", derive(Resource))]
pub struct Server<E: Copy + Eq + Hash + Send + Sync> {
    // Config
    server_config: ServerConfig,
    protocol: Protocol,
    io: Io,
    heartbeat_timer: Timer,
    timeout_timer: Timer,
    ping_timer: Timer,
    handshake_manager: HandshakeManager,
    // Users
    users: BigMap<UserKey, User>,
    user_connections: HashMap<SocketAddr, Connection<E>>,
    validated_users: HashMap<SocketAddr, UserKey>,
    // Rooms
    rooms: BigMap<RoomKey, Room<E>>,
    // Entities
    entity_room_map: HashMap<E, RoomKey>,
    entity_scope_map: EntityScopeMap<E>,
    global_world_manager: GlobalWorldManager<E>,
    // Events
    incoming_events: Events<E>,
    // Ticks
    time_manager: TimeManager,
}

impl<E: Copy + Eq + Hash + Send + Sync> Server<E> {
    /// Create a new Server
    pub fn new<P: Into<Protocol>>(server_config: ServerConfig, protocol: P) -> Self {
        let mut protocol: Protocol = protocol.into();
        protocol.lock();

        let time_manager = TimeManager::new(protocol.tick_interval);

        let io = Io::new(
            &server_config.connection.bandwidth_measure_duration,
            &protocol.compression,
        );

        Server {
            // Config
            server_config: server_config.clone(),
            protocol,
            // Connection
            io,
            heartbeat_timer: Timer::new(server_config.connection.heartbeat_interval),
            timeout_timer: Timer::new(server_config.connection.disconnection_timeout_duration),
            ping_timer: Timer::new(server_config.ping.ping_interval),
            handshake_manager: HandshakeManager::new(server_config.require_auth),
            // Users
            users: BigMap::new(),
            user_connections: HashMap::new(),
            validated_users: HashMap::new(),
            // Rooms
            rooms: BigMap::new(),
            // Entities
            entity_room_map: HashMap::new(),
            entity_scope_map: EntityScopeMap::new(),
            global_world_manager: GlobalWorldManager::new(),
            // Events
            incoming_events: Events::new(),
            // Ticks
            time_manager,
        }
    }

    /// Listen at the given addresses
    pub fn listen<S: Into<Box<dyn Socket>>>(&mut self, socket: S) {
        let boxed_socket: Box<dyn Socket> = socket.into();
        let (packet_sender, packet_receiver) = boxed_socket.listen();
        self.io.load(packet_sender, packet_receiver);
    }

    /// Returns whether or not the Server has initialized correctly and is
    /// listening for Clients
    pub fn is_listening(&self) -> bool {
        self.io.is_loaded()
    }

    /// Returns socket config
    pub fn socket_config(&self) -> &SocketConfig {
        &self.protocol.socket
    }

    /// Must be called regularly, maintains connection to and receives messages
    /// from all Clients
    pub fn receive<W: WorldMutType<E>>(&mut self, world: W) -> Events<E> {
        // Need to run this to maintain connection with all clients, and receive packets
        // until none left
        self.maintain_socket(world);

        // tick event
        if self.time_manager.recv_server_tick() {
            self.incoming_events
                .push_tick(self.time_manager.current_tick());
        }

        // return all received messages and reset the buffer
        std::mem::replace(&mut self.incoming_events, Events::<E>::new())
    }

    // Connections

    /// Accepts an incoming Client User, allowing them to establish a connection
    /// with the Server
    pub fn accept_connection(&mut self, user_key: &UserKey) {
        let Some(user) = self.users.get(user_key) else {
            warn!("unknown user is finalizing connection...");
            return;
        };

        // send validate response
        let writer = self.handshake_manager.write_validate_response();
        if self
            .io
            .send_packet(&user.address, writer.to_packet())
            .is_err()
        {
            // TODO: pass this on and handle above
            warn!(
                "Server Error: Cannot send validate response packet to {}",
                &user.address
            );
        }

        self.validated_users.insert(user.address, *user_key);
    }

    fn finalize_connection(&mut self, user_key: &UserKey) {
        let Some(user) = self.users.get(user_key) else {
            warn!("unknown user is finalizing connection...");
            return;
        };
        let new_connection = Connection::new(
            &self.server_config.connection,
            &self.server_config.ping,
            &user.address,
            user_key,
            &self.protocol.channel_kinds,
            &self.global_world_manager,
        );

        // send connect response
        let writer = self.handshake_manager.write_connect_response();
        if self
            .io
            .send_packet(&user.address, writer.to_packet())
            .is_err()
        {
            // TODO: pass this on and handle above
            warn!(
                "Server Error: Cannot send connect response packet to {}",
                &user.address
            );
        }

        self.user_connections.insert(user.address, new_connection);
        if self.io.bandwidth_monitor_enabled() {
            self.io.register_client(&user.address);
        }
        self.incoming_events.push_connection(user_key);
    }

    /// Rejects an incoming Client User, terminating their attempt to establish
    /// a connection with the Server
    pub fn reject_connection(&mut self, user_key: &UserKey) {
        if let Some(user) = self.users.get(user_key) {
            // send connect reject response
            let writer = self.handshake_manager.write_reject_response();
            if self
                .io
                .send_packet(&user.address, writer.to_packet())
                .is_err()
            {
                // TODO: pass this on and handle above
                warn!(
                    "Server Error: Cannot send auth rejection packet to {}",
                    &user.address
                );
            }
            //
        }
        self.user_delete(user_key);
    }

    // Messages

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    pub fn send_message<C: Channel, M: Message>(&mut self, user_key: &UserKey, message: &M) {
        let cloned_message = M::clone_box(message);
        self.send_message_inner(user_key, &ChannelKind::of::<C>(), cloned_message);
    }

    /// Queues up an Message to be sent to the Client associated with a given
    /// UserKey
    fn send_message_inner(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        let channel_settings = self.protocol.channel_kinds.channel(channel_kind);

        if !channel_settings.can_send_to_client() {
            panic!("Cannot send message to Client on this Channel");
        }

        if let Some(user) = self.users.get(user_key) {
            if let Some(connection) = self.user_connections.get_mut(&user.address) {
                let mut converter = EntityConverterMut::new(
                    &self.global_world_manager,
                    &mut connection.base.local_world_manager,
                );
                let message = MessageContainer::from_write(message_box, &mut converter);
                connection.base.message_manager.send_message(
                    &self.protocol.message_kinds,
                    &mut converter,
                    channel_kind,
                    message,
                );
            }
        }
    }

    /// Sends a message to all connected users using a given channel
    pub fn broadcast_message<C: Channel, M: Message>(&mut self, message: &M) {
        let cloned_message = M::clone_box(message);
        self.broadcast_message_inner(&ChannelKind::of::<C>(), cloned_message);
    }

    fn broadcast_message_inner(
        &mut self,
        channel_kind: &ChannelKind,
        message_box: Box<dyn Message>,
    ) {
        self.user_keys().iter().for_each(|user_key| {
            self.send_message_inner(user_key, channel_kind, message_box.clone())
        })
    }

    pub fn receive_tick_buffer_messages(&mut self, tick: &Tick) -> TickBufferMessages {
        let mut tick_buffer_messages = TickBufferMessages::new();
        for (_user_address, connection) in self.user_connections.iter_mut() {
            // receive messages from anyone
            connection.tick_buffer_messages(tick, &mut tick_buffer_messages);
        }
        tick_buffer_messages
    }

    // Updates

    /// Used to evaluate whether, given a User & Entity that are in the
    /// same Room, said Entity should be in scope for the given User.
    ///
    /// While Rooms allow for a very simple scope to which an Entity can belong,
    /// this provides complete customization for advanced scopes.
    ///
    /// Return a collection of Entity Scope Sets, being a unique combination of
    /// a related Room, User, and Entity, used to determine which Entities to
    /// replicate to which Users
    pub fn scope_checks(&self) -> Vec<(RoomKey, UserKey, E)> {
        let mut list: Vec<(RoomKey, UserKey, E)> = Vec::new();

        // TODO: precache this, instead of generating a new list every call
        // likely this is called A LOT
        for (room_key, room) in self.rooms.iter() {
            for user_key in room.user_keys() {
                for entity in room.entities() {
                    list.push((room_key, *user_key, *entity));
                }
            }
        }

        list
    }

    /// Sends all update messages to all Clients. If you don't call this
    /// method, the Server will never communicate with it's connected
    /// Clients
    pub fn send_all_updates<W: WorldRefType<E>>(&mut self, world: W) {
        let now = Instant::now();

        // update entity scopes
        self.update_entity_scopes(&world);

        // loop through all connections, send packet
        let mut user_addresses: Vec<SocketAddr> = self.user_connections.keys().copied().collect();

        // shuffle order of connections in order to avoid priority among users
        fastrand::shuffle(&mut user_addresses);

        for user_address in user_addresses {
            let connection = self.user_connections.get_mut(&user_address).unwrap();

            connection.send_packets(
                &self.protocol,
                &now,
                &mut self.io,
                &world,
                &self.global_world_manager,
                &self.time_manager,
            );
        }
    }

    // Entities

    pub fn enable_replication(&mut self, entity: &E) {
        self.spawn_entity_inner(&entity);
    }

    pub fn disable_replication(&mut self, entity: &E) {
        // Despawn from connections and inner tracking
        self.despawn_entity_worldless(entity);
    }

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&mut self, mut world: W) -> EntityMut<E, W> {
        let entity = world.spawn_entity();
        self.spawn_entity_inner(&entity);

        EntityMut::new(self, world, &entity)
    }

    /// Creates a new Entity with a specific id
    pub fn spawn_entity_at(&mut self, entity: &E) {
        self.spawn_entity_inner(entity);
    }

    fn spawn_entity_inner(&mut self, entity: &E) {
        self.global_world_manager
            .spawn_entity_record(entity, EntityOwner::Server);
    }

    /// Retrieves an EntityRef that exposes read-only operations for the
    /// Entity.
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
        if world.has_entity(entity) {
            return EntityMut::new(self, world, entity);
        }
        panic!("No Entity exists for given Key!");
    }

    /// Gets a Vec of all Entities in the given World
    pub fn entities<W: WorldRefType<E>>(&self, world: W) -> Vec<E> {
        world.entities()
    }

    pub fn entity_owner(&self, entity: &E) -> EntityOwner {
        if let Some(owner) = self.global_world_manager.entity_owner(entity) {
            return owner;
        }
        return EntityOwner::Local;
    }

    // Users

    /// Returns whether or not a User exists for the given RoomKey
    pub fn user_exists(&self, user_key: &UserKey) -> bool {
        self.users.contains_key(user_key)
    }

    /// Retrieves an UserRef that exposes read-only operations for the User
    /// associated with the given UserKey.
    /// Panics if the user does not exist.
    pub fn user(&self, user_key: &UserKey) -> UserRef<E> {
        if self.users.contains_key(user_key) {
            return UserRef::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Retrieves an UserMut that exposes read and write operations for the User
    /// associated with the given UserKey.
    /// Returns None if the user does not exist.
    pub fn user_mut(&mut self, user_key: &UserKey) -> UserMut<E> {
        if self.users.contains_key(user_key) {
            return UserMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    /// Return a list of all currently connected Users' keys
    pub fn user_keys(&self) -> Vec<UserKey> {
        let mut output = Vec::new();

        for (user_key, _) in self.users.iter() {
            output.push(user_key);
        }

        output
    }

    /// Get the number of Users currently connected
    pub fn users_count(&self) -> usize {
        self.users.len()
    }

    /// Returns a UserScopeMut, which is used to include/exclude Entities for a
    /// given User
    pub fn user_scope(&mut self, user_key: &UserKey) -> UserScopeMut<E> {
        if self.users.contains_key(user_key) {
            return UserScopeMut::new(self, user_key);
        }
        panic!("No User exists for given Key!");
    }

    // Rooms

    /// Creates a new Room on the Server and returns a corresponding RoomMut,
    /// which can be used to add users/entities to the room or retrieve its
    /// key
    pub fn make_room(&mut self) -> RoomMut<E> {
        let new_room = Room::new();
        let room_key = self.rooms.insert(new_room);
        RoomMut::new(self, &room_key)
    }

    /// Returns whether or not a Room exists for the given RoomKey
    pub fn room_exists(&self, room_key: &RoomKey) -> bool {
        self.rooms.contains_key(room_key)
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room(&self, room_key: &RoomKey) -> RoomRef<E> {
        if self.rooms.contains_key(room_key) {
            return RoomRef::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Retrieves an RoomMut that exposes read and write operations for the
    /// Room associated with the given RoomKey.
    /// Panics if the room does not exist.
    pub fn room_mut(&mut self, room_key: &RoomKey) -> RoomMut<E> {
        if self.rooms.contains_key(room_key) {
            return RoomMut::new(self, room_key);
        }
        panic!("No Room exists for given Key!");
    }

    /// Return a list of all the Server's Rooms' keys
    pub fn room_keys(&self) -> Vec<RoomKey> {
        let mut output = Vec::new();

        for (key, _) in self.rooms.iter() {
            output.push(key);
        }

        output
    }

    /// Get a count of how many Rooms currently exist
    pub fn rooms_count(&self) -> usize {
        self.rooms.len()
    }

    // Ticks

    /// Gets the current tick of the Server
    pub fn current_tick(&self) -> Tick {
        return self.time_manager.current_tick();
    }

    /// Gets the current average tick duration of the Server
    pub fn average_tick_duration(&self) -> Duration {
        self.time_manager.average_tick_duration()
    }

    // Bandwidth monitoring
    pub fn outgoing_bandwidth_total(&mut self) -> f32 {
        self.io.outgoing_bandwidth_total()
    }

    pub fn incoming_bandwidth_total(&mut self) -> f32 {
        self.io.incoming_bandwidth_total()
    }

    pub fn outgoing_bandwidth_to_client(&mut self, address: &SocketAddr) -> f32 {
        self.io.outgoing_bandwidth_to_client(address)
    }

    pub fn incoming_bandwidth_from_client(&mut self, address: &SocketAddr) -> f32 {
        self.io.incoming_bandwidth_from_client(address)
    }

    // Ping
    /// Gets the average Round Trip Time measured to the given User's Client
    pub fn rtt(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.users.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address) {
                return Some(connection.ping_manager.rtt_average);
            }
        }
        None
    }

    /// Gets the average Jitter measured in connection to the given User's
    /// Client
    pub fn jitter(&self, user_key: &UserKey) -> Option<f32> {
        if let Some(user) = self.users.get(user_key) {
            if let Some(connection) = self.user_connections.get(&user.address) {
                return Some(connection.ping_manager.jitter_average);
            }
        }
        None
    }

    // Crate-Public methods

    //// Entities

    /// Despawns the Entity, if it exists.
    /// This will also remove all of the Entity’s Components.
    /// Panics if the Entity does not exist.
    pub(crate) fn despawn_entity<W: WorldMutType<E>>(&mut self, world: &mut W, entity: &E) {
        if !world.has_entity(entity) {
            panic!("attempted to de-spawn nonexistent entity");
        }

        // Delete from world
        world.despawn_entity(entity);

        self.despawn_entity_worldless(entity);
    }

    pub fn despawn_entity_worldless(&mut self, entity: &E) {
        // TODO: we can make this more efficient in the future by caching which Entities
        // are in each User's scope
        for (_, connection) in self.user_connections.iter_mut() {
            //remove entity from user connection
            connection.base.host_world_manager.despawn_entity(entity);
        }

        // Delete scope
        self.entity_scope_map.remove_entity(entity);

        // Delete room cache entry
        if let Some(room_key) = self.entity_room_map.remove(entity) {
            if let Some(room) = self.rooms.get_mut(&room_key) {
                room.remove_entity(entity);
            }
        }

        // Remove from ECS Record
        self.global_world_manager
            .remove_entity_diff_handlers(entity);
        self.global_world_manager.remove_entity_record(entity);
    }

    //// Entity Scopes

    /// Remove all entities from a User's scope
    pub(crate) fn user_scope_remove_user(&mut self, user_key: &UserKey) {
        self.entity_scope_map.remove_user(user_key);
    }

    pub(crate) fn user_scope_set_entity(
        &mut self,
        user_key: &UserKey,
        entity: &E,
        is_contained: bool,
    ) {
        self.entity_scope_map
            .insert(*user_key, *entity, is_contained);
    }

    //// Components

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

        self.insert_new_component_into_entity_scopes(entity, &component_kind);

        // update in world manager
        self.global_world_manager
            .insert_component_record(entity, &component_kind);
        self.global_world_manager
            .insert_component_diff_handler(entity, component);
    }

    fn insert_new_component_into_entity_scopes(
        &mut self,
        entity: &E,
        component_kind: &ComponentKind,
    ) {
        // add component to connections already tracking entity
        for (_, connection) in self.user_connections.iter_mut() {
            // insert component into user's connection
            if connection.base.host_world_manager.host_has_entity(entity) {
                connection
                    .base
                    .host_world_manager
                    .insert_component(entity, &component_kind);
            }
        }
    }

    /// Removes a Component from an Entity
    pub(crate) fn remove_component<R: Replicate, W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
    ) -> Option<R> {
        self.remove_component_worldless(entity, &ComponentKind::of::<R>());

        // remove from world
        world.remove_component::<R>(entity)
    }

    // This intended to be used by adapter crates, do not use this as it will not update the world
    pub fn remove_component_worldless(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.remove_component_from_all_connections(entity, component_kind);

        // cleanup all other loose ends
        self.global_world_manager
            .remove_component_record(entity, &component_kind);
        self.global_world_manager
            .remove_component_diff_handler(entity, &component_kind);
    }

    fn remove_component_from_all_connections(
        &mut self,
        entity: &E,
        component_kind: &ComponentKind,
    ) {
        // TODO: should be able to make this more efficient by caching for every Entity
        // which scopes they are part of
        for (_, connection) in self.user_connections.iter_mut() {
            // remove component from user connection
            connection
                .base
                .host_world_manager
                .remove_component(entity, &component_kind);
        }
    }

    //// Users

    /// Get a User's Socket Address, given the associated UserKey
    pub(crate) fn user_address(&self, user_key: &UserKey) -> Option<SocketAddr> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.address);
        }
        None
    }

    /// Returns an iterator of all the keys of the [`Room`]s the User belongs to
    pub(crate) fn user_room_keys(&self, user_key: &UserKey) -> Option<Iter<RoomKey>> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.room_keys());
        }
        return None;
    }

    /// Get an count of how many Rooms the given User is inside
    pub(crate) fn user_rooms_count(&self, user_key: &UserKey) -> Option<usize> {
        if let Some(user) = self.users.get(user_key) {
            return Some(user.room_count());
        }
        return None;
    }

    pub(crate) fn user_disconnect<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        if self.protocol.client_authoritative_entities {
            self.despawn_all_remote_entities(user_key, world);
        }
        let user = self.user_delete(user_key);
        self.incoming_events.push_disconnection(user_key, user);
    }

    /// All necessary cleanup, when they're actually gone...
    pub(crate) fn despawn_all_remote_entities<W: WorldMutType<E>>(
        &mut self,
        user_key: &UserKey,
        world: &mut W,
    ) {
        let Some(user) = self.users.get(user_key) else {
            panic!("Attempting to despawn entities for a nonexistent user");
        };
        let Some (connection) = self.user_connections.get_mut(&user.address) else {
            panic!("Attempting to despawn entities on a nonexistent connection");
        };

        let entity_events = connection.base.despawn_all_remote_entities(world);
        let response_events = self
            .incoming_events
            .receive_entity_events(user_key, entity_events);
        self.process_response_events(world, user_key, response_events);
    }

    pub(crate) fn user_delete(&mut self, user_key: &UserKey) -> User {
        let Some(user) = self.users.remove(user_key) else {
            panic!("Attempting to delete non-existant user!");
        };

        self.user_connections.remove(&user.address);
        self.validated_users.remove(&user.address);
        self.entity_scope_map.remove_user(user_key);
        self.handshake_manager.delete_user(&user.address);

        // Clean up all user data
        for room_key in user.room_keys() {
            self.rooms
                .get_mut(room_key)
                .unwrap()
                .unsubscribe_user(user_key);
        }

        // remove from bandwidth monitor
        if self.io.bandwidth_monitor_enabled() {
            self.io.deregister_client(&user.address);
        }

        return user;
    }

    //// Rooms

    /// Deletes the Room associated with a given RoomKey on the Server.
    /// Returns true if the Room existed.
    pub(crate) fn room_destroy(&mut self, room_key: &RoomKey) -> bool {
        self.room_remove_all_entities(room_key);

        if self.rooms.contains_key(room_key) {
            // TODO: what else kind of cleanup do we need to do here? Scopes?

            // actually remove the room from the collection
            let room = self.rooms.remove(room_key).unwrap();
            for user_key in room.user_keys() {
                self.users.get_mut(user_key).unwrap().uncache_room(room_key);
            }

            true
        } else {
            false
        }
    }

    //////// users

    /// Returns whether or not an User is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_user(&self, room_key: &RoomKey, user_key: &UserKey) -> bool {
        if let Some(room) = self.rooms.get(room_key) {
            return room.has_user(user_key);
        }
        false
    }

    /// Add an User to a Room, given the appropriate RoomKey & UserKey
    /// Entities will only ever be in-scope for Users which are in a
    /// Room with them
    pub(crate) fn room_add_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(user) = self.users.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.subscribe_user(user_key);
                user.cache_room(room_key);
            }
        }
    }

    /// Removes a User from a Room
    pub(crate) fn room_remove_user(&mut self, room_key: &RoomKey, user_key: &UserKey) {
        if let Some(user) = self.users.get_mut(user_key) {
            if let Some(room) = self.rooms.get_mut(room_key) {
                room.unsubscribe_user(user_key);
                user.uncache_room(room_key);
            }
        }
    }

    /// Get a count of Users in a given Room
    pub(crate) fn room_users_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(room_key) {
            return room.users_count();
        }
        0
    }

    /// Returns an iterator of the [`UserKey`] for Users that belong in the Room
    pub(crate) fn room_user_keys(&self, room_key: &RoomKey) -> impl Iterator<Item = &UserKey> {
        let iter = if let Some(room) = self.rooms.get(room_key) {
            Some(room.user_keys())
        } else {
            None
        };
        iter.into_iter().flatten()
    }

    /// Sends a message to all connected users in a given Room using a given channel
    pub(crate) fn room_broadcast_message(
        &mut self,
        channel_kind: &ChannelKind,
        room_key: &RoomKey,
        message_box: Box<dyn Message>,
    ) {
        if let Some(room) = self.rooms.get(room_key) {
            let user_keys: Vec<UserKey> = room.user_keys().cloned().collect();
            for user_key in &user_keys {
                self.send_message_inner(user_key, channel_kind, message_box.clone())
            }
        }
    }

    //////// entities

    /// Returns whether or not an Entity is currently in a specific Room, given
    /// their keys.
    pub(crate) fn room_has_entity(&self, room_key: &RoomKey, entity: &E) -> bool {
        let Some(actual_room_key) = self.entity_room_map.get(entity) else {
            return false;
        };
        return *room_key == *actual_room_key;
    }

    /// Add an Entity to a Room associated with the given RoomKey.
    /// Entities will only ever be in-scope for Users which are in a Room with
    /// them.
    pub(crate) fn room_add_entity(&mut self, room_key: &RoomKey, entity: &E) {
        let mut is_some = false;
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.add_entity(entity);
            is_some = true;
        }
        if !is_some {
            return;
        }
        if self.entity_room_map.contains_key(entity) {
            panic!("Entity already belongs to a Room! Remove the Entity from the Room before adding it to a new Room.");
        }
        self.entity_room_map.insert(*entity, *room_key);
    }

    /// Remove an Entity from a Room, associated with the given RoomKey
    pub(crate) fn room_remove_entity(&mut self, room_key: &RoomKey, entity: &E) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.remove_entity(entity);
            self.entity_room_map.remove(entity);
        }
    }

    /// Remove all Entities from a Room, associated with the given RoomKey
    fn room_remove_all_entities(&mut self, room_key: &RoomKey) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            let entities: Vec<E> = room.entities().copied().collect();
            for entity in entities {
                room.remove_entity(&entity);
                self.entity_room_map.remove(&entity);
            }
        }
    }

    /// Get a count of Entities in a given Room
    pub(crate) fn room_entities_count(&self, room_key: &RoomKey) -> usize {
        if let Some(room) = self.rooms.get(room_key) {
            return room.entities_count();
        }
        0
    }

    // Private methods

    /// Maintain connection with a client and read all incoming packet data
    fn maintain_socket<W: WorldMutType<E>>(&mut self, mut world: W) {
        self.handle_disconnects(&mut world);
        self.handle_heartbeats();
        self.handle_pings();

        let mut addresses: HashSet<SocketAddr> = HashSet::new();
        // receive socket events
        loop {
            match self.io.recv_reader() {
                Ok(Some((address, owned_reader))) => {
                    let mut reader = owned_reader.borrow();

                    // Read header
                    let Ok(header) = StandardHeader::de(&mut reader) else {
                        // Received a malformed packet
                        // TODO: increase suspicion against packet sender
                        continue;
                    };

                    let Ok(should_continue) = self.maintain_handshake(&address, &header, &mut reader) else {
                        warn!("Server Error: cannot read malformed packet");
                        continue;
                    };
                    if should_continue {
                        continue;
                    }

                    addresses.insert(address);

                    if self
                        .read_packet(&address, &header, &mut reader, &mut world)
                        .is_err()
                    {
                        warn!("Server Error: cannot read malformed packet");
                        continue;
                    }
                }
                Ok(None) => {
                    // No more packets, break loop
                    break;
                }
                Err(error) => {
                    self.incoming_events
                        .push_error(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }

        for address in addresses {
            self.process_packets(&address, &mut world);
        }
    }

    fn maintain_handshake(
        &mut self,
        address: &SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
    ) -> Result<bool, SerdeErr> {
        // Handshake stuff
        match header.packet_type {
            PacketType::ClientChallengeRequest => {
                if let Ok(writer) = self.handshake_manager.recv_challenge_request(reader) {
                    if self.io.send_packet(&address, writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send challenge response packet to {}",
                            &address
                        );
                    }
                }
                return Ok(true);
            }
            PacketType::ClientValidateRequest => {
                match self.handshake_manager.recv_validate_request(
                    &self.protocol.message_kinds,
                    address,
                    reader,
                ) {
                    HandshakeResult::Success(auth_message_opt) => {
                        if self.validated_users.contains_key(address) {
                            // send validate response
                            let writer = self.handshake_manager.write_validate_response();
                            if self.io.send_packet(address, writer.to_packet()).is_err() {
                                // TODO: pass this on and handle above
                                warn!("Server Error: Cannot send validate success response packet to {}", &address);
                            };
                        } else {
                            let user = User::new(*address);
                            let user_key = self.users.insert(user);

                            if let Some(auth_message) = auth_message_opt {
                                self.incoming_events.push_auth(&user_key, auth_message);
                            } else {
                                self.accept_connection(&user_key);
                            }
                        }
                    }
                    HandshakeResult::Invalid => {
                        // do nothing
                    }
                }
                return Ok(true);
            }
            PacketType::ClientConnectRequest => {
                if self.user_connections.contains_key(address) {
                    // send connect response
                    let writer = self.handshake_manager.write_connect_response();
                    if self.io.send_packet(address, writer.to_packet()).is_err() {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send connect success response packet to {}",
                            address
                        );
                    };
                    //
                } else {
                    let user_key = *self
                        .validated_users
                        .get(address)
                        .expect("should be a user by now, from validation step");
                    self.finalize_connection(&user_key);
                }
                return Ok(true);
            }
            PacketType::Ping => {
                let response = self.time_manager.process_ping(reader).unwrap();
                // send packet
                if self.io.send_packet(address, response.to_packet()).is_err() {
                    // TODO: pass this on and handle above
                    warn!("Server Error: Cannot send pong packet to {}", address);
                };
                if let Some(connection) = self.user_connections.get_mut(address) {
                    connection.base.mark_sent();
                }
                return Ok(true);
            }
            _ => {}
        }

        return Ok(false);
    }

    fn read_packet<W: WorldMutType<E>>(
        &mut self,
        address: &SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
        world: &mut W,
    ) -> Result<(), SerdeErr> {
        // Packets requiring established connection
        let Some(connection) = self.user_connections.get_mut(address) else {
            return Ok(());
        };

        // Mark that we've heard from the client
        connection.base.mark_heard();

        // Process incoming header
        connection.process_incoming_header(header);

        match header.packet_type {
            PacketType::Data => {
                // read client tick
                let client_tick = Tick::de(reader)?;

                let server_tick = self.time_manager.current_tick();

                // process data
                connection.read_packet(
                    &self.protocol,
                    server_tick,
                    client_tick,
                    reader,
                    &mut self.global_world_manager,
                )?;
            }
            PacketType::Disconnect => {
                if self
                    .handshake_manager
                    .verify_disconnect_request(connection, reader)
                {
                    let user_key = connection.user_key;
                    self.user_disconnect(&user_key, world);
                }
            }
            PacketType::Heartbeat => {
                // already marked heard above
            }
            PacketType::Pong => {
                // read client tick
                connection
                    .ping_manager
                    .process_pong(&self.time_manager, reader);
            }
            _ => {}
        }

        return Ok(());
    }

    fn process_packets<W: WorldMutType<E>>(&mut self, address: &SocketAddr, world: &mut W) {
        // Packets requiring established connection
        if let Some((user_key, response_events)) = {
            let Some(connection) = self.user_connections.get_mut(address) else {
                return;
            };
            connection
                .process_packets(
                    &self.protocol,
                    &mut self.global_world_manager,
                    world,
                    &mut self.incoming_events,
                )
                .map(|events| (connection.user_key, events))
        } {
            self.process_response_events(world, &user_key, response_events);
        }
    }

    fn process_response_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        user_key: &UserKey,
        response_events: Vec<EntityResponseEvent<E>>,
    ) {
        for response_event in response_events {
            match response_event {
                EntityResponseEvent::SpawnEntity(entity) => {
                    self.global_world_manager
                        .spawn_entity_record(&entity, EntityOwner::Client(*user_key));
                }
                EntityResponseEvent::DespawnEntity(entity) => {
                    if self.global_world_manager.entity_is_public(&entity) {
                        self.despawn_entity_worldless(&entity);
                        info!("server process public EntityResponseEvent::DespawnEntity");
                    } else {
                        self.global_world_manager.remove_entity_record(&entity);
                    }
                }
                EntityResponseEvent::InsertComponent(entity, component_kind) => {
                    self.global_world_manager
                        .insert_component_record(&entity, &component_kind);
                    if self.global_world_manager.entity_is_public(&entity) {
                        world.component_publish(
                            &self.global_world_manager,
                            &entity,
                            &component_kind,
                        );
                        self.insert_new_component_into_entity_scopes(&entity, &component_kind);
                        info!("server process public EntityResponseEvent::InsertComponent");
                    }
                }
                EntityResponseEvent::RemoveComponent(entity, component_kind) => {
                    if self.global_world_manager.entity_is_public(&entity) {
                        self.remove_component_worldless(&entity, &component_kind);
                        info!("server process public EntityResponseEvent::RemoveComponent");
                    } else {
                        self.global_world_manager
                            .remove_component_record(&entity, &component_kind);
                    }
                }
            }
        }
    }

    fn handle_disconnects<W: WorldMutType<E>>(&mut self, world: &mut W) {
        // disconnects
        if self.timeout_timer.ringing() {
            self.timeout_timer.reset();

            let mut user_disconnects: Vec<UserKey> = Vec::new();

            for (_, connection) in &mut self.user_connections.iter_mut() {
                // user disconnects
                if connection.base.should_drop() {
                    user_disconnects.push(connection.user_key);
                    continue;
                }
            }

            for user_key in user_disconnects {
                self.user_disconnect(&user_key, world);
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
                    // Don't try to refactor this to self.internal_send, doesn't seem to
                    // work cause of iter_mut()
                    let mut writer = BitWriter::new();

                    // write header
                    connection
                        .base
                        .write_header(PacketType::Heartbeat, &mut writer);

                    // write server tick
                    self.time_manager.current_tick().ser(&mut writer);

                    // write server tick instant
                    self.time_manager.current_tick_instant().ser(&mut writer);

                    // send packet
                    if self
                        .io
                        .send_packet(user_address, writer.to_packet())
                        .is_err()
                    {
                        // TODO: pass this on and handle above
                        warn!(
                            "Server Error: Cannot send heartbeat packet to {}",
                            user_address
                        );
                    }
                    connection.base.mark_sent();
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
                    connection.base.write_header(PacketType::Ping, &mut writer);

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
                        // TODO: pass this on and handle above
                        warn!("Server Error: Cannot send ping packet to {}", user_address);
                    }
                    connection.base.mark_sent();
                }
            }
        }
    }

    // Entity Scopes

    fn update_entity_scopes<W: WorldRefType<E>>(&mut self, world: &W) {
        for (_, room) in self.rooms.iter_mut() {
            while let Some((removed_user, removed_entity)) = room.pop_entity_removal_queue() {
                if let Some(user) = self.users.get(&removed_user) {
                    if let Some(connection) = self.user_connections.get_mut(&user.address) {
                        // TODO: evaluate whether the Entity really needs to be despawned!
                        // What if the Entity shares another Room with this User? It shouldn't be despawned!

                        //remove entity from user connection
                        connection
                            .base
                            .host_world_manager
                            .despawn_entity(&removed_entity);
                    }
                }
            }

            // TODO: we should be able to cache these tuples of keys to avoid building a new
            // list each time
            for user_key in room.user_keys() {
                if let Some(user) = self.users.get(user_key) {
                    if let Some(connection) = self.user_connections.get_mut(&user.address) {
                        for entity in room.entities() {
                            if world.has_entity(entity) {
                                if self
                                    .global_world_manager
                                    .entity_is_public_and_owned_by_user(user_key, entity)
                                {
                                    // entity is owned by client, but it is public, so we don't need to replicate it
                                    continue;
                                }

                                let currently_in_scope =
                                    connection.base.host_world_manager.host_has_entity(entity);

                                let should_be_in_scope = if let Some(in_scope) =
                                    self.entity_scope_map.get(user_key, entity)
                                {
                                    *in_scope
                                } else {
                                    false
                                };

                                if should_be_in_scope {
                                    if !currently_in_scope {
                                        //temp//remove!//
                                        if self.global_world_manager.entity_is_public(entity) {
                                            info!("Server: replicating Public Entity to User")
                                        }
                                        //

                                        let component_kinds = self
                                            .global_world_manager
                                            .component_kinds(entity)
                                            .unwrap();
                                        // add entity & components to the connections local scope
                                        connection.base.host_world_manager.init_entity(
                                            &mut connection.base.local_world_manager,
                                            entity,
                                            component_kinds,
                                        );
                                    }
                                } else if currently_in_scope {
                                    // remove entity from the connections local scope
                                    connection.base.host_world_manager.despawn_entity(entity);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E> for Server<E> {
    fn global_entity_to_entity(
        &self,
        global_entity: &GlobalEntity,
    ) -> Result<E, EntityDoesNotExistError> {
        self.global_world_manager
            .global_entity_to_entity(global_entity)
    }

    fn entity_to_global_entity(&self, entity: &E) -> Result<GlobalEntity, EntityDoesNotExistError> {
        self.global_world_manager.entity_to_global_entity(entity)
    }
}
