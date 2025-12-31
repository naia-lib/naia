use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

use log::{debug, warn};

use naia_shared::{
    BaseConnection, BitReader, BitWriter, ChannelKinds, ComponentKind, ComponentKinds,
    ConnectionConfig, EntityAndGlobalEntityConverter, EntityCommand, EntityEvent, GlobalEntity,
    GlobalEntitySpawner, HostType, Instant, MessageIndex, MessageKinds, PacketType, Protocol,
    Serde, SerdeErr, StandardHeader, Tick, Timer, WorldMutType, WorldRefType,
};

use crate::{
    connection::{
        io::Io,
        jitter_buffer::{JitterBuffer, JitterBufferType},
        tick_buffer_sender::TickBufferSender,
        time_manager::TimeManager,
    },
    request::{GlobalRequestManager, GlobalResponseManager},
    world::global_world_manager::GlobalWorldManager,
    world_events::WorldEvents,
};

pub struct Connection {
    pub base: BaseConnection,
    pub timeout_timer: Timer,
    pub time_manager: TimeManager,
    pub tick_buffer: TickBufferSender,
    // Request/Response
    pub global_request_manager: GlobalRequestManager,
    pub global_response_manager: GlobalResponseManager,
    /// Small buffer when receiving updates (entity actions, entity updates) from the server
    /// to make sure we receive them in order
    jitter_buffer: JitterBuffer,
}

impl Connection {
    pub fn new(
        connection_config: &ConnectionConfig,
        channel_kinds: &ChannelKinds,
        time_manager: TimeManager,
        global_world_manager: &GlobalWorldManager,
        jitter_buffer_type: JitterBufferType,
    ) -> Self {
        let mut connection = Self {
            timeout_timer: Timer::new(connection_config.disconnection_timeout_duration),
            base: BaseConnection::new(
                connection_config,
                &None,
                HostType::Client,
                0,
                channel_kinds,
                global_world_manager,
            ),
            time_manager,
            tick_buffer: TickBufferSender::new(channel_kinds),
            jitter_buffer: JitterBuffer::new(jitter_buffer_type),
            global_request_manager: GlobalRequestManager::new(),
            global_response_manager: GlobalResponseManager::new(),
        };

        let existing_entities = global_world_manager.entities();
        for entity in existing_entities {
            let component_kinds = global_world_manager.component_kinds(&entity).unwrap();
            connection
                .base
                .world_manager
                .host_init_entity(&entity, component_kinds);
        }

        connection
    }

    // Incoming data

    /// Record that a message has been sent (to prevent needing to send a
    /// heartbeat)
    pub fn mark_sent(&mut self) {
        self.base.mark_sent();
    }

    pub fn mark_heard(&mut self) {
        self.timeout_timer.reset()
    }

    pub fn should_drop(&self) -> bool {
        self.timeout_timer.ringing()
    }

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        eprintln!(
            "[ack] CLIENT RX server_ack={:?} server_ack_bits={:?}",
            header.sender_ack_index, header.sender_ack_bitfield
        );
        self.base
            .process_incoming_header(header, &mut [&mut self.tick_buffer]);
    }

    pub fn buffer_data_packet(
        &mut self,
        incoming_tick: &Tick,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        // Use debug logging instead of println to reduce noise
        debug!(
            "[CLIENT_CONN] buffer_data_packet: Buffering packet for server_tick={:?}",
            incoming_tick
        );
        self.jitter_buffer
            .add_item(*incoming_tick, reader.to_owned());
        Ok(())
    }

    /// Read the packets (raw bits) from the jitter buffer that correspond to the
    /// `receiving_tick`. Reads packets, storing necessary data into an internal buffer
    pub fn read_buffered_packets(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        component_kinds: &ComponentKinds,
    ) -> Result<(), SerdeErr> {
        let receiving_tick = self.time_manager.client_receiving_tick;
        debug!(
            "[CLIENT_CONN] read_buffered_packets: Reading packets for receiving_tick={:?}",
            receiving_tick
        );

        let mut packets_read = 0;
        while let Some((server_tick, owned_reader)) = self.jitter_buffer.pop_item(receiving_tick) {
            packets_read += 1;
            debug!(
                "[CLIENT_CONN] read_buffered_packets: Reading packet server_tick={:?}",
                server_tick
            );
            let mut reader = owned_reader.borrow();

            self.base.read_packet(
                channel_kinds,
                message_kinds,
                component_kinds,
                &server_tick,
                true,
                &mut reader,
            )?;
        }
        if packets_read > 0 {
            debug!(
                "[CLIENT_CONN] read_buffered_packets: Read {} packets",
                packets_read
            );
        }

        Ok(())
    }

    /// Receive & process messages / entity actions / entity updates and emit events for them
    pub fn process_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldMutType<E>>(
        &mut self,
        global_entity_map: &mut dyn GlobalEntitySpawner<E>,
        global_world_manager: &mut GlobalWorldManager,
        protocol: &Protocol,
        world: &mut W,
        now: &Instant,
        incoming_events: &mut WorldEvents<E>,
    ) -> Vec<EntityEvent> {
        // Receive Message Events
        let (entity_converter, entity_waitlist) =
            self.base.world_manager.get_message_processor_helpers();
        let messages = self.base.message_manager.receive_messages(
            &protocol.message_kinds,
            now,
            entity_converter,
            entity_waitlist,
        );
        for (channel_kind, messages) in messages {
            for message in messages {
                incoming_events.push_message(&channel_kind, message);
            }
        }

        // Receive Request and Response Events
        let (requests, responses) = self.base.message_manager.receive_requests_and_responses();
        // Requests
        for (channel_kind, requests) in requests {
            for (local_response_id, request) in requests {
                let global_response_id = self
                    .global_response_manager
                    .create_response_id(&channel_kind, &local_response_id);
                incoming_events.push_request(&channel_kind, global_response_id, request);
            }
        }
        // Responses
        for (global_request_id, response) in responses {
            self.global_request_manager
                .receive_response(&global_request_id, response);
        }

        // Receive World Events
        self.base.world_manager.take_incoming_events(
            global_entity_map,
            global_world_manager,
            &protocol.component_kinds,
            world,
            now,
        )
    }

    // Outgoing data

    /// Collect and send any outgoing packets from client to server
    pub fn send_packets<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
    ) {
        let rtt_millis = self.time_manager.rtt();
        self.base.collect_messages(now, &rtt_millis);
        self.tick_buffer.collect_messages(
            &self.time_manager.client_sending_tick,
            &self.time_manager.server_receivable_tick,
        );
        let (mut host_world_events, mut update_events) = self
            .base
            .world_manager
            .take_outgoing_events(now, &rtt_millis, world, converter, global_world_manager);

        let mut any_sent = false;
        let mut iteration = 0;
        loop {
            eprintln!(
                "[probe] send_packets loop iteration={}, host_events.len()={}",
                iteration,
                host_world_events.len()
            );
            if self.send_packet(
                protocol,
                now,
                io,
                world,
                converter,
                global_world_manager,
                &mut host_world_events,
                &mut update_events,
            ) {
                any_sent = true;
                eprintln!("[probe] send_packet returned TRUE (packet sent)");
            } else {
                eprintln!("[probe] send_packet returned FALSE (breaking loop)");
                break;
            }
            iteration += 1;
            if iteration > 5 {
                eprintln!("[probe] LOOP SAFETY BREAK after {} iterations", iteration);
                break;
            }
        }
        if any_sent {
            self.mark_sent();
        }
    }

    // Sends packet and returns whether or not a packet was sent
    fn send_packet<E: Copy + Eq + Hash + Send + Sync, W: WorldRefType<E>>(
        &mut self,
        protocol: &Protocol,
        now: &Instant,
        io: &mut Io,
        world: &W,
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        global_world_manager: &GlobalWorldManager,
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) -> bool {
        if !host_world_events.is_empty()
            || !update_events.is_empty()
            || self.base.message_manager.has_outgoing_messages()
            || self.tick_buffer.has_messages()
        {
            let writer = self.write_packet(
                protocol,
                now,
                world,
                converter,
                global_world_manager,
                host_world_events,
                update_events,
            );

            // send packet
            let packet = writer.to_packet();
            eprintln!("[rep_probe] client send frame");
            if io.send_packet(packet).is_err() {
                // TODO: pass this on and handle above
                warn!("Client Error: Cannot send data packet to Server");
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
        host_world_events: &mut VecDeque<(MessageIndex, EntityCommand)>,
        update_events: &mut HashMap<GlobalEntity, HashSet<ComponentKind>>,
    ) -> BitWriter {
        let next_packet_index = self.base.next_packet_index();

        let mut writer = BitWriter::new();

        // Reserve bits we know will be required to finish the message:
        // 1. Tick buffer finish bit
        // 2. Messages finish bit
        // 3. Updates finish bit
        // 4. Actions finish bit
        writer.reserve_bits(4);

        // write header
        self.base.write_header(PacketType::Data, &mut writer);

        // write client tick
        let client_tick: Tick = self.time_manager.client_sending_tick;
        client_tick.ser(&mut writer);

        let mut has_written = false;

        // write tick buffered messages
        self.tick_buffer.write_messages(
            &protocol,
            global_world_manager,
            &mut self.base,
            &mut writer,
            next_packet_index,
            &client_tick,
            &mut has_written,
        );

        // write common parts of packet (messages & world events)
        self.base.write_packet(
            &protocol.channel_kinds,
            &protocol.message_kinds,
            &protocol.component_kinds,
            now,
            &mut writer,
            next_packet_index,
            world,
            entity_converter,
            global_world_manager,
            &mut has_written,
            protocol.client_authoritative_entities,
            host_world_events,
            update_events,
        );

        writer
    }

    pub fn process_received_commands(&mut self) {
        self.base.world_manager.process_delivered_commands();
    }
}
