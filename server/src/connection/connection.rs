use std::collections::{HashMap, HashSet, VecDeque};
use std::{hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{
    BaseConnection, BigMapKey, BitReader, BitWriter, ChannelKinds, ComponentKind, ComponentKinds,
    ConnectionConfig, EntityAndGlobalEntityConverter, EntityCommand, EntityEvent, GlobalEntity,
    GlobalEntitySpawner, HostType, Instant, MessageIndex, MessageKinds, PacketType, Serde,
    SerdeErr, StandardHeader, Tick, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        io::Io, ping_config::PingConfig, ping_manager::PingManager,
        tick_buffer_messages::TickBufferMessages, tick_buffer_receiver::TickBufferReceiver,
    },
    events::WorldEvents,
    request::{GlobalRequestManager, GlobalResponseManager},
    time_manager::TimeManager,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
};

pub struct Connection {
    pub address: SocketAddr,
    pub user_key: UserKey,
    pub base: BaseConnection,
    pub ping_manager: PingManager,
    tick_buffer: TickBufferReceiver,
    pub manual_disconnect: bool,
}

impl Connection {
    pub fn new(
        connection_config: &ConnectionConfig,
        ping_config: &PingConfig,
        user_address: &SocketAddr,
        user_key: &UserKey,
        channel_kinds: &ChannelKinds,
        global_world_manager: &GlobalWorldManager,
    ) -> Self {
        Self {
            address: *user_address,
            user_key: *user_key,
            base: BaseConnection::new(
                connection_config,
                &Some(*user_address),
                HostType::Server,
                user_key.to_u64(),
                channel_kinds,
                global_world_manager,
            ),
            ping_manager: PingManager::new(ping_config),
            tick_buffer: TickBufferReceiver::new(channel_kinds),
            manual_disconnect: false,
        }
    }

    // Incoming Data

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.base.process_incoming_header(header, &mut []);
    }

    /// Read packet data received from a client, storing necessary data in an internal buffer
    pub fn read_packet(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        server_tick: Tick,
        client_tick: Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read tick-buffered messages
        self.tick_buffer.read_messages(
            channel_kinds,
            message_kinds,
            &server_tick,
            &client_tick,
            self.base.world_manager.entity_converter(),
            reader,
        )?;

        // read common parts of packet (messages & world events)
        self.base.read_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            &client_tick,
            client_authoritative_entities,
            reader,
        )?;

        return Ok(());
    }

    /// Receive & process stored packet data
    pub fn process_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        client_authoritative_entities: bool,
        now: &Instant,
        global_entity_map: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &mut GlobalWorldManager,
        global_request_manager: &mut GlobalRequestManager,
        global_response_manager: &mut GlobalResponseManager,
        world: &mut W,
        incoming_events: &mut WorldEvents<E>,
    ) -> Vec<EntityEvent> {
        // Receive Message Events
        let (entity_converter, entity_waitlist) =
            self.base.world_manager.get_message_processor_helpers();
        let messages = self.base.message_manager.receive_messages(
            message_kinds,
            now,
            entity_converter,
            entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&self.user_key, &channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.base.message_manager.receive_requests_and_responses();
        // Requests
        for (channel_kind, requests) in requests {
            for (local_response_id, request) in requests {
                let global_response_id = global_response_manager.create_response_id(
                    &self.user_key,
                    &channel_kind,
                    &local_response_id,
                );
                incoming_events.push_request(
                    &self.user_key,
                    &channel_kind,
                    global_response_id,
                    request,
                );
            }
        }
        // Responses
        for (global_request_id, response) in responses {
            global_request_manager.receive_response(&global_request_id, response);
        }

        // Receive World Events
        if client_authoritative_entities {
            return self.base.world_manager.take_incoming_events(
                global_entity_map,
                global_world_manager,
                component_kinds,
                world,
                now,
            );
        } else {
            return Vec::new();
        }
    }

    pub fn tick_buffer_messages(&mut self, tick: &Tick, messages: &mut TickBufferMessages) {
        let channel_messages = self.tick_buffer.receive_messages(tick);
        for (channel_kind, received_messages) in channel_messages {
            for message in received_messages {
                messages.push_message(&self.user_key, &channel_kind, message);
            }
        }
    }

    // Outgoing data
    pub fn send_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        io: &mut Io,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
    ) {
        let rtt_millis = self.ping_manager.rtt_average;
        self.base.collect_messages(now, &rtt_millis);
        let (mut host_world_events, mut update_events) = self
            .base
            .world_manager
            .take_outgoing_events(now, &rtt_millis, world, converter, global_world_manager);

        let mut any_sent = false;
        loop {
            if self.send_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                io,
                world,
                converter,
                global_world_manager,
                time_manager,
                &mut host_world_events,
                &mut update_events,
            ) {
                any_sent = true;
            } else {
                break;
            }
        }
        if any_sent {
            self.base.mark_sent();
        }
    }

    /// Send any message, component actions and component updates to the client
    /// Will split the data into multiple packets.
    fn send_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        io: &mut Io,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) -> bool {
        let has_messages = self.base.message_manager.has_outgoing_messages();
        let has_events = !host_world_events.is_empty() || !update_events.is_empty();
        
        if has_events || has_messages {
            let writer = self.write_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                now,
                world,
                entity_converter,
                global_world_manager,
                time_manager,
                host_world_events,
                update_events,
            );

            // send packet
            if io.send_packet(&self.address, writer.to_packet()).is_err() {
                // TODO: pass this on and handle above
                warn!("Server Error: Cannot send data packet to {}", &self.address);
            }

            return true;
        }

        false
    }

    fn write_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
        now: &Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) -> BitWriter {
        let next_packet_index = self.base.next_packet_index();

        let mut writer = BitWriter::new();

        // Reserve bits we know will be required to finish the message:
        // 1. Messages finish bit
        // 2. Updates finish bit
        // 3. Actions finish bit
        writer.reserve_bits(3);

        // write header
        self.base.write_header(PacketType::Data, &mut writer);

        // write server tick
        let tick = time_manager.current_tick();
        tick.ser(&mut writer);

        // write server tick instant
        time_manager.current_tick_instant().ser(&mut writer);

        // write common data packet
        let mut has_written = false;
        self.base.write_packet(
            channel_kinds,
            message_kinds,
            component_kinds,
            now,
            &mut writer,
            next_packet_index,
            world,
            entity_converter,
            global_world_manager,
            &mut has_written,
            true,
            host_world_events,
            update_events,
        );

        writer
    }

    pub fn process_received_commands(&mut self) {
        self.base.world_manager.process_delivered_commands();
    }
}
