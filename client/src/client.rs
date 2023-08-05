use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use log::{info, warn};

#[cfg(feature = "bevy_support")]
use bevy_ecs::prelude::Resource;

use naia_shared::{
    BitWriter, Channel, ChannelKind, ComponentKind, EntityAndGlobalEntityConverter,
    EntityAndLocalEntityConverter, EntityAuthStatus, EntityConverterMut, EntityDoesNotExistError,
    EntityEventMessage, EntityResponseEvent, FakeEntityConverter, GameInstant, GlobalEntity,
    GlobalWorldManagerType, Instant, Message, MessageContainer, PacketType, Protocol, RemoteEntity,
    Replicate, Serde, SharedGlobalWorldManager, SocketConfig, StandardHeader, SystemChannel, Tick,
    WorldMutType, WorldRefType,
};

use super::{client_config::ClientConfig, error::NaiaClientError, events::Events};
use crate::{
    connection::{
        base_time_manager::BaseTimeManager,
        connection::Connection,
        handshake_manager::{HandshakeManager, HandshakeResult},
        io::Io,
    },
    transport::Socket,
    world::{
        entity_mut::EntityMut, entity_owner::EntityOwner, entity_ref::EntityRef,
        global_world_manager::GlobalWorldManager,
    },
    ReplicationConfig,
};

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
    waitlist_messages: VecDeque<(ChannelKind, Box<dyn Message>)>,
    // World
    global_world_manager: GlobalWorldManager<E>,
    // Events
    incoming_events: Events<E>,
    // Hacky
    queued_entity_auth_release_messages: Vec<E>,
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
            waitlist_messages: VecDeque::new(),
            // World
            global_world_manager: GlobalWorldManager::new(),
            // Events
            incoming_events: Events::new(),
            // Hacky
            queued_entity_auth_release_messages: Vec::new(),
        }
    }

    /// Set the auth object to use when setting up a connection with the Server
    pub fn auth<M: Message>(&mut self, auth: M) {
        self.handshake_manager
            .set_auth_message(MessageContainer::from_write(
                Box::new(auth),
                &mut FakeEntityConverter,
            ));
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

        self.send_queued_auth_release_messages();

        let mut response_events = None;

        // all other operations
        if let Some(connection) = &mut self.server_connection {
            if connection.base.should_drop() || self.manual_disconnect {
                self.disconnect_with_events(&mut world);
                return std::mem::take(&mut self.incoming_events);
            }

            let (receiving_tick_happened, sending_tick_happened) =
                connection.time_manager.collect_ticks();

            if let Some((prev_receiving_tick, current_receiving_tick)) = receiving_tick_happened {
                // read packets on tick boundary, de-jittering
                if connection
                    .read_buffered_packets(&self.protocol, &mut self.global_world_manager)
                    .is_err()
                {
                    // TODO: Except for cosmic radiation .. Server should never send a malformed packet .. handle this
                    warn!("Error reading from buffered packet!");
                }

                // receive packets, process into events
                response_events = Some(connection.process_packets(
                    &mut self.global_world_manager,
                    &self.protocol.component_kinds,
                    &mut world,
                    &mut self.incoming_events,
                ));

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

                // collect waiting auth release messages
                if let Some(mut entities) = connection
                    .base
                    .host_world_manager
                    .world_channel
                    .collect_auth_release_messages()
                {
                    self.queued_entity_auth_release_messages
                        .append(&mut entities);
                }

                // send packets
                connection.send_packets(
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

        if let Some(events) = response_events {
            self.process_response_events(&mut world, events);
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
        } else {
            self.waitlist_messages
                .push_back((channel_kind.clone(), message_box));
        }
    }

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
            let mut converter = EntityConverterMut::new(
                &self.global_world_manager,
                &mut connection.base.local_world_manager,
            );
            let message = MessageContainer::from_write(message_box, &mut converter);
            connection
                .tick_buffer
                .send_message(tick, channel_kind, message);
        }
    }

    // Entities

    /// Creates a new Entity and returns an EntityMut which can be used for
    /// further operations on the Entity
    pub fn spawn_entity<W: WorldMutType<E>>(&mut self, mut world: W) -> EntityMut<E, W> {
        self.check_client_authoritative_allowed();

        let entity = world.spawn_entity();
        self.spawn_entity_inner(&entity);

        EntityMut::new(self, world, &entity)
    }

    /// Creates a new Entity with a specific id
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
            return EntityRef::new(self, world, entity);
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
    pub fn entity_replication_config(&self, entity: &E) -> Option<ReplicationConfig> {
        self.check_client_authoritative_allowed();
        self.global_world_manager.entity_replication_config(entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn configure_entity_replication<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        config: ReplicationConfig,
    ) {
        self.check_client_authoritative_allowed();
        if !self.global_world_manager.has_entity(entity) {
            panic!("Entity is not yet replicating. Be sure to call `enable_replication` or `spawn_entity` on the Client, before configuring replication.");
        }
        let entity_owner = self.global_world_manager.entity_owner(entity).unwrap();
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
            .entity_replication_config(entity)
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
                        self.publish_entity(entity, true);
                    }
                    ReplicationConfig::Delegated => {
                        // private -> delegated
                        self.publish_entity(entity, true);
                        self.entity_enable_delegation(world, entity, true);
                    }
                }
            }
            ReplicationConfig::Public => {
                match next_config {
                    ReplicationConfig::Private => {
                        // public -> private
                        self.unpublish_entity(entity, true);
                    }
                    ReplicationConfig::Public => {
                        panic!("This should not be possible.");
                    }
                    ReplicationConfig::Delegated => {
                        // public -> delegated
                        self.entity_enable_delegation(world, entity, true);
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
    pub fn entity_authority_status(&self, entity: &E) -> Option<EntityAuthStatus> {
        self.check_client_authoritative_allowed();

        self.global_world_manager.entity_authority_status(entity)
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_request_authority(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();

        // 1. Set local authority status for Entity
        let success = self.global_world_manager.entity_request_authority(entity);
        if success {
            // Reserve Host Entity
            let Some(connection) = &mut self.server_connection else {
                return;
            };
            let new_host_entity = connection
                .base
                .local_world_manager
                .host_reserve_entity(entity);

            // 2. Send request to Server
            let message = EntityEventMessage::new_request_authority(
                &self.global_world_manager,
                entity,
                new_host_entity,
            );
            self.send_message::<SystemChannel, EntityEventMessage>(&message);
        }
    }

    /// This is used only for Hecs/Bevy adapter crates, do not use otherwise!
    pub fn entity_release_authority(&mut self, entity: &E) {
        self.check_client_authoritative_allowed();

        // 1. Set local authority status for Entity
        let success = self.global_world_manager.entity_release_authority(entity);
        if success {
            let Some(connection) = &mut self.server_connection else {
                return;
            };
            let send_release_message = connection
                .base
                .host_world_manager
                .world_channel
                .entity_release_authority(entity);
            if send_release_message {
                self.send_entity_release_auth_message(entity);
            }
        }
    }

    fn send_entity_release_auth_message(&mut self, entity: &E) {
        // 3. Send request to Server
        let message = EntityEventMessage::new_release_authority(&self.global_world_manager, entity);
        self.send_message::<SystemChannel, EntityEventMessage>(&message);
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
        if !self.global_world_manager.has_entity(entity) {
            warn!("attempting to despawn entity that has already been despawned?");
            return;
        }

        // check whether we have authority to despawn this entity
        if let Some(owner) = self.global_world_manager.entity_owner(entity) {
            if owner.is_server() {
                if !self.global_world_manager.entity_is_delegated(entity) {
                    panic!("attempting to despawn entity that is not yet delegated. Delegation needs some time to be confirmed by the Server, so check that a despawn is possible by calling `commands.entity(..).replication_config(..).is_delegated()` first.");
                }
                if self.global_world_manager.entity_authority_status(entity)
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

        // if entity is delegated, convert over
        if self.global_world_manager.entity_is_delegated(entity) {
            let accessor = self.global_world_manager.get_entity_auth_accessor(entity);
            component.enable_delegation(&accessor, None)
        }
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

    pub(crate) fn publish_entity(&mut self, entity: &E, client_is_origin: bool) {
        if client_is_origin {
            let message = EntityEventMessage::new_publish(&self.global_world_manager, entity);
            self.send_message::<SystemChannel, EntityEventMessage>(&message);
        } else {
            if self.global_world_manager.entity_replication_config(entity)
                != Some(ReplicationConfig::Private)
            {
                panic!("Server can only publish Private entities");
            }
        }
        self.global_world_manager.entity_publish(entity);
        // don't need to publish the Entity/Component via the World here, because Remote entities work the same whether they are published or not
    }

    pub(crate) fn unpublish_entity(&mut self, entity: &E, client_is_origin: bool) {
        if client_is_origin {
            let message = EntityEventMessage::new_unpublish(&self.global_world_manager, entity);
            self.send_message::<SystemChannel, EntityEventMessage>(&message);
        } else {
            if self.global_world_manager.entity_replication_config(entity)
                != Some(ReplicationConfig::Public)
            {
                panic!("Server can only unpublish Public entities");
            }
        }
        self.global_world_manager.entity_unpublish(entity);
        // don't need to publish the Entity/Component via the World here, because Remote entities work the same whether they are published or not
    }

    pub(crate) fn entity_enable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        client_is_origin: bool,
    ) {
        // this should happen BEFORE the world entity/component has been translated over to Delegated
        self.global_world_manager
            .entity_register_auth_for_delegation(entity);

        if client_is_origin {
            // send message to server
            let message =
                EntityEventMessage::new_enable_delegation(&self.global_world_manager, entity);
            self.send_message::<SystemChannel, EntityEventMessage>(&message);
        } else {
            self.entity_complete_delegation(world, entity);
            self.global_world_manager
                .entity_update_authority(entity, EntityAuthStatus::Available);
        }
    }

    fn entity_complete_delegation<W: WorldMutType<E>>(&mut self, world: &mut W, entity: &E) {
        world.entity_enable_delegation(&self.global_world_manager, &entity);

        // this should happen AFTER the world entity/component has been translated over to Delegated
        self.global_world_manager.entity_enable_delegation(&entity);
    }

    pub(crate) fn entity_disable_delegation<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        entity: &E,
        client_is_origin: bool,
    ) {
        info!("client.entity_disable_delegation");
        if client_is_origin {
            panic!("Cannot disable delegation from Client. Server owns all delegated Entities.");
        }

        self.global_world_manager.entity_disable_delegation(entity);
        world.entity_disable_delegation(entity);

        // Despawn Entity in Host connection
        self.despawn_entity_worldless(entity)
    }

    pub(crate) fn entity_update_authority(
        &mut self,
        entity: &E,
        new_auth_status: EntityAuthStatus,
    ) {
        let old_auth_status = self
            .global_world_manager
            .entity_authority_status(entity)
            .unwrap();

        self.global_world_manager
            .entity_update_authority(entity, new_auth_status);

        // info!(
        //     "<-- Received Entity Update Authority message! {:?} -> {:?}",
        //     old_auth_status, new_auth_status
        // );

        // Updated Host Manager
        match (old_auth_status, new_auth_status) {
            (EntityAuthStatus::Requested, EntityAuthStatus::Granted) => {
                // Granted Authority

                let Some(connection) = &mut self.server_connection else {
                    return;
                };
                // Migrate Entity from Remote -> Host connection
                let component_kinds = self.global_world_manager.component_kinds(entity).unwrap();
                connection.base.host_world_manager.track_remote_entity(
                    &mut connection.base.local_world_manager,
                    entity,
                    component_kinds,
                );

                // push outgoing event
                self.incoming_events.push_auth_grant(*entity);
            }
            (EntityAuthStatus::Releasing, EntityAuthStatus::Available)
            | (EntityAuthStatus::Granted, EntityAuthStatus::Available) => {
                // Lost Authority

                // Remove Entity from Host connection
                let Some(connection) = &mut self.server_connection else {
                    return;
                };
                connection
                    .base
                    .host_world_manager
                    .untrack_remote_entity(&mut connection.base.local_world_manager, entity);

                // push outgoing event
                self.incoming_events.push_auth_reset(*entity);
            }
            (EntityAuthStatus::Available, EntityAuthStatus::Denied) => {
                // push outgoing event
                self.incoming_events.push_auth_deny(*entity);
            }
            (EntityAuthStatus::Denied, EntityAuthStatus::Available) => {
                // push outgoing event
                self.incoming_events.push_auth_reset(*entity);
            }
            (EntityAuthStatus::Releasing, EntityAuthStatus::Granted) => {
                // granted auth response arrived while we are releasing auth!
                self.global_world_manager
                    .entity_update_authority(entity, EntityAuthStatus::Available);

                // get rid of reserved host entity
                let Some(connection) = &mut self.server_connection else {
                    return;
                };
                connection
                    .base
                    .local_world_manager
                    .remove_reserved_host_entity(entity);
            }
            (EntityAuthStatus::Available, EntityAuthStatus::Available) => {
                // auth was released before it was granted, continue as normal
            }
            (_, _) => {
                panic!(
                    "-- Entity updated authority, not handled -- {:?} -> {:?}",
                    old_auth_status, new_auth_status
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
                            self.on_connect();

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
                .write_header(PacketType::Heartbeat, &mut writer);

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

        let remote_entities = connection.base.remote_entities();
        let entity_events = SharedGlobalWorldManager::<E>::despawn_all_entities(
            world,
            &self.global_world_manager,
            remote_entities,
        );
        let response_events = self.incoming_events.receive_world_events(entity_events);
        self.process_response_events(world, response_events);
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

    fn process_response_events<W: WorldMutType<E>>(
        &mut self,
        world: &mut W,
        response_events: Vec<EntityResponseEvent<E>>,
    ) {
        for response_event in response_events {
            match response_event {
                EntityResponseEvent::SpawnEntity(entity) => {
                    self.global_world_manager.remote_spawn_entity(&entity);
                    let Some(connection) = self.server_connection.as_mut() else {
                        panic!("Client is disconnected!");
                    };
                    let local_entity = connection
                        .base
                        .local_world_manager
                        .entity_to_remote_entity(&entity)
                        .unwrap();
                    connection
                        .base
                        .remote_world_manager
                        .on_entity_channel_opened(&local_entity);
                }
                EntityResponseEvent::DespawnEntity(entity) => {
                    self.global_world_manager.remove_entity_record(&entity);
                }
                EntityResponseEvent::InsertComponent(entity, component_kind) => {
                    self.global_world_manager
                        .remote_insert_component(&entity, &component_kind);
                }
                EntityResponseEvent::RemoveComponent(entity, component_kind) => {
                    self.global_world_manager
                        .remote_remove_component(&entity, &component_kind);
                }
                EntityResponseEvent::PublishEntity(entity) => {
                    self.publish_entity(&entity, false);
                    self.incoming_events.push_publish(entity);
                }
                EntityResponseEvent::UnpublishEntity(entity) => {
                    self.unpublish_entity(&entity, false);
                    self.incoming_events.push_unpublish(entity);
                }
                EntityResponseEvent::EnableDelegationEntity(entity) => {
                    self.entity_enable_delegation(world, &entity, false);

                    // send response
                    let message = EntityEventMessage::new_enable_delegation_response(
                        &self.global_world_manager,
                        &entity,
                    );
                    self.send_message::<SystemChannel, EntityEventMessage>(&message);
                }
                EntityResponseEvent::EnableDelegationEntityResponse(_) => {
                    panic!("Client should never receive an EnableDelegationEntityResponse event");
                }
                EntityResponseEvent::DisableDelegationEntity(entity) => {
                    self.entity_disable_delegation(world, &entity, false);
                }
                EntityResponseEvent::EntityRequestAuthority(_entity, _remote_entity) => {
                    panic!("Client should never receive an EntityRequestAuthority event");
                }
                EntityResponseEvent::EntityReleaseAuthority(_entity) => {
                    panic!("Client should never receive an EntityReleaseAuthority event");
                }
                EntityResponseEvent::EntityUpdateAuthority(entity, new_auth_status) => {
                    self.entity_update_authority(&entity, new_auth_status);
                }
                EntityResponseEvent::EntityMigrateResponse(world_entity, remote_entity) => {
                    self.entity_complete_delegation(world, &world_entity);
                    self.add_redundant_remote_entity_to_host(&world_entity, remote_entity);

                    self.global_world_manager
                        .entity_update_authority(&world_entity, EntityAuthStatus::Granted);

                    self.incoming_events.push_auth_grant(world_entity);
                }
            }
        }
    }

    pub fn add_redundant_remote_entity_to_host(
        &mut self,
        world_entity: &E,
        remote_entity: RemoteEntity,
    ) {
        let Some(connection) = self.server_connection.as_mut() else {
            panic!("Client is disconnected!");
        };

        // Local World Manager now tracks the Entity by it's Remote Entity
        connection
            .base
            .local_world_manager
            .insert_remote_entity(world_entity, remote_entity);

        // Remote world reader needs to track remote entity too
        let component_kinds = self
            .global_world_manager
            .component_kinds(world_entity)
            .unwrap();
        connection
            .base
            .remote_world_reader
            .track_hosts_redundant_remote_entity(&remote_entity, component_kinds);
    }

    fn send_queued_auth_release_messages(&mut self) {
        if self.queued_entity_auth_release_messages.is_empty() {
            return;
        }
        let entities = std::mem::take(&mut self.queued_entity_auth_release_messages);
        for entity in entities {
            self.send_entity_release_auth_message(&entity);
        }
    }
}

impl<E: Copy + Eq + Hash + Send + Sync> EntityAndGlobalEntityConverter<E> for Client<E> {
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
