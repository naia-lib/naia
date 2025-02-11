use std::{any::Any, hash::Hash, net::SocketAddr};

use log::warn;

use naia_shared::{BaseConnection, BigMapKey, BitReader, BitWriter, ChannelKind, ChannelKinds, ConnectionConfig, EntityAndGlobalEntityConverter, EntityEventMessage, EntityResponseEvent, GlobalEntitySpawner, HostType, HostWorldEvents, Instant, PacketType, Protocol, Serde, SerdeErr, StandardHeader, SystemChannel, Tick, WorldMutType, WorldRefType};

use crate::request::{GlobalRequestManager, GlobalResponseManager};
use crate::{
    connection::{
        io::Io, ping_config::PingConfig, tick_buffer_messages::TickBufferMessages,
        tick_buffer_receiver::TickBufferReceiver,
    },
    events::Events,
    time_manager::TimeManager,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
};

use super::ping_manager::PingManager;

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
                &Some(*user_address),
                HostType::Server,
                user_key.to_u64(),
                connection_config,
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
        protocol: &Protocol,
        server_tick: Tick,
        client_tick: Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // read tick-buffered messages
        self.tick_buffer.read_messages(
            protocol,
            &server_tick,
            &client_tick,
            self.base.local_world_manager.entity_converter(),
            reader,
        )?;

        // read common parts of packet (messages & world events)
        self.base.read_packet(
            protocol,
            &client_tick,
            protocol.client_authoritative_entities,
            reader,
        )?;

        return Ok(());
    }

    /// Receive & process stored packet data
    pub fn process_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        global_entity_map: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &mut GlobalWorldManager,
        global_request_manager: &mut GlobalRequestManager,
        global_response_manager: &mut GlobalResponseManager,
        world: &mut W,
        incoming_events: &mut Events<E>,
    ) -> Vec<EntityResponseEvent> {
        let mut response_events = Vec::new();
        // Receive Message Events
        let messages = self.base.message_manager.receive_messages(
            &protocol.message_kinds,
            now,
            self.base.local_world_manager.entity_converter(),
            &mut self.base.remote_world_manager.entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            if channel_kind == ChannelKind::of::<SystemChannel>() {
                for message in messages {
                    let Some(event_message) = Box::<dyn Any + 'static>::downcast::<
                        EntityEventMessage,
                    >(message.to_boxed_any())
                    .ok()
                    .map(|boxed_m| *boxed_m) else {
                        panic!("Received unknown message over SystemChannel!");
                    };
                    match event_message.entity.get_inner() {
                        Some(global_entity) => {
                            response_events.push(event_message.action.to_response_event(&global_entity));
                        }
                        None => {
                            warn!(
                                "Received `{:?}` with no Entity over SystemChannel!",
                                event_message.action
                            );
                        }
                    };
                }
            } else {
                for message in messages {
                    incoming_events.push_message(&self.user_key, &channel_kind, message);
                }
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
        if protocol.client_authoritative_entities {
            let remote_events = self.base.remote_world_reader.take_incoming_events();
            let world_events = self.base.remote_world_manager.process_world_events(
                global_entity_map,
                global_world_manager,
                &mut self.base.local_world_manager,
                &protocol.component_kinds,
                world,
                now,
                remote_events,
            );
            response_events
                .extend(incoming_events.receive_entity_events(global_entity_map.to_converter(), &self.user_key, world_events));
        }

        return response_events;
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
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
    ) {
        let rtt_millis = self.ping_manager.rtt_average;
        self.base.collect_messages(now, &rtt_millis);
        let mut host_world_events = self.base.host_world_manager.take_outgoing_events(
            world,
            converter,
            global_world_manager,
            now,
            &rtt_millis,
        );

        let mut any_sent = false;
        loop {
            if self.send_packet(
                protocol,
                now,
                io,
                world,
                converter,
                global_world_manager,
                time_manager,
                &mut host_world_events,
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
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut HostWorldEvents,
    ) -> bool {
        if host_world_events.has_events() || self.base.message_manager.has_outgoing_messages() {
            let writer = self.write_packet(
                protocol,
                now,
                world,
                entity_converter,
                global_world_manager,
                time_manager,
                host_world_events,
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
        protocol: &Protocol,
        now: &Instant,
        world: &W,
        entity_converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        time_manager: &TimeManager,
        host_world_events: &mut HostWorldEvents,
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
            &protocol,
            now,
            &mut writer,
            next_packet_index,
            world,
            entity_converter,
            global_world_manager,
            &mut has_written,
            true,
            host_world_events,
        );

        writer
    }
}
