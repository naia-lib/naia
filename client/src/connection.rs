use std::{hash::Hash, net::SocketAddr, collections::VecDeque};

use naia_client_socket::Packet;

use naia_shared::{BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketReader,
                  PacketType, Protocolize, ReplicateSafe, SequenceNumber, StandardHeader,
                  WorldMutType};

use super::{
    entity_action::EntityAction, entity_manager::EntityManager,
    packet_writer::PacketWriter, ping_manager::PingManager,
    tick_manager::TickManager, tick_queue::TickQueue,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    base_connection: BaseConnection<P>,
    entity_manager: EntityManager<P, E>,
    ping_manager: PingManager,
    outgoing_entity_messages: VecDeque<(E, P)>,
    jitter_buffer: TickQueue<(u16, Box<[u8]>)>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return Connection {
            base_connection: BaseConnection::new(address, connection_config),
            entity_manager: EntityManager::new(),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.ping_sample_size,
            ),
            outgoing_entity_messages: VecDeque::new(),
            jitter_buffer: TickQueue::new(),
        };
    }

    pub fn get_outgoing_packet(&mut self, host_tick_opt: Option<u16>) -> Option<Box<[u8]>> {
        if self.base_connection.has_outgoing_messages() || !self.outgoing_entity_messages.is_empty() {
            let mut writer = PacketWriter::new();

            // Entity Messages
            while let Some((entity, message)) = self.outgoing_entity_messages.pop_front() {
                if !writer.write_entity_message(
                    &self.entity_manager,
                    &entity,
                    &message,
                ) {
                    self.outgoing_entity_messages.push_front((entity, message));
                    break;
                }
            }

            // Messages
            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_message) =
                self.base_connection.pop_outgoing_message(next_packet_index)
            {
                if !writer.write_message(&popped_message) {
                    self.base_connection
                        .unpop_outgoing_message(next_packet_index, popped_message);
                    break;
                }
            }

            // Add header
            if writer.has_bytes() {
                // Get bytes from writer

                let out_bytes = writer.get_bytes();

                // Add header to it
                let payload = self.process_outgoing_header(
                    host_tick_opt,
                    self.base_connection.get_last_received_tick(),
                    PacketType::Data,
                    &out_bytes,
                );
                return Some(payload);
            }
        }

        return None;
    }

    pub fn process_incoming_data<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        packet_index: u16,
        manifest: &Manifest<P>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Message => {
                    self.base_connection
                        .process_message_data(&mut reader, manifest, packet_index);
                }
                ManagerType::Entity => {
                    self.entity_manager.process_data(
                        world,
                        manifest,
                        packet_index,
                        &mut reader,
                    );
                }
                _ => {}
            }
        }
    }

    pub fn buffer_data_packet(
        &mut self,
        incoming_tick: u16,
        incoming_packet_index: u16,
        incoming_payload: &Box<[u8]>,
    ) {
        self.jitter_buffer.add_item(
            incoming_tick,
            (incoming_packet_index, incoming_payload.clone()),
        );
    }

    // Pass-through methods to underlying Entity Manager
    pub fn get_incoming_entity_action(&mut self) -> Option<EntityAction<P, E>> {
        return self.entity_manager.pop_incoming_message();
    }

    /// Reads buffered incoming data on the appropriate tick boundary
    pub fn frame_begin<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        tick_manager: &mut TickManager,
    ) -> bool {
        if tick_manager.mark_frame() {
            // then we apply all received updates to components at once
            let receiving_tick = tick_manager.receiving_tick();
            self.process_buffered_packet(world, manifest, receiving_tick);
            return true;
        }
        return false;
    }

    /// Reads buffered incoming data, regardless of any ticks
    pub fn tickless_read_incoming<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
    ) {
        self.process_buffered_packet(world, manifest, 0);
    }

    fn process_buffered_packet<W: WorldMutType<P, E>>(
        &mut self,
        world: &mut W,
        manifest: &Manifest<P>,
        target_tick: u16,
    ) {
        while let Some((_, packet_index, data_packet)) =
            self.get_buffered_data_packet(target_tick)
        {
            self.process_incoming_data(world, packet_index, manifest, &data_packet);
        }
    }

    // Pass-through methods to underlying Connection

    pub fn mark_sent(&mut self) {
        return self.base_connection.mark_sent();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        return self.base_connection.should_send_heartbeat();
    }

    pub fn mark_heard(&mut self) {
        return self.base_connection.mark_heard();
    }

    pub fn should_drop(&self) -> bool {
        return self.base_connection.should_drop();
    }

    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        tick_manager_opt: Option<&mut TickManager>,
    ) {
        if let Some(tick_manager) = tick_manager_opt {
            tick_manager.record_server_tick(
                header.host_tick(),
                self.ping_manager.get_ping(),
                self.ping_manager.get_jitter(),
            );
        }
        self.base_connection
            .process_incoming_header(header, &mut None);
    }

    pub fn process_outgoing_header(
        &mut self,
        host_tick: Option<u16>,
        last_received_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self.base_connection.process_outgoing_header(
            host_tick,
            last_received_tick,
            packet_type,
            payload,
        );
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.base_connection.get_next_packet_index();
    }

    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        return self
            .base_connection
            .send_message(message, guaranteed_delivery);
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(&mut self, entity: &E, message: &R, client_tick: u16) {
        let message_protocol = message.protocol_copy();
        return self.outgoing_entity_messages.push_back((*entity, message_protocol));
    }

    pub fn get_incoming_message(&mut self) -> Option<P> {
        return self.base_connection.get_incoming_message();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        self.base_connection.get_last_received_tick()
    }

    // Ping related
    pub fn should_send_ping(&self) -> bool {
        return self.ping_manager.should_send_ping();
    }

    pub fn get_ping_payload(&mut self) -> Packet {
        let payload = self.ping_manager.get_ping_payload();
        return Packet::new_raw(payload);
    }

    pub fn process_pong(&mut self, pong_payload: &[u8]) {
        self.ping_manager.process_pong(pong_payload);
    }

    pub fn get_rtt(&self) -> f32 {
        return self.ping_manager.get_rtt();
    }

    pub fn get_jitter(&self) -> f32 {
        return self.ping_manager.get_jitter();
    }

    // Private methods

    fn get_buffered_data_packet(&mut self, current_tick: u16) -> Option<(u16, u16, Box<[u8]>)> {
        if let Some((tick, (index, payload))) = self.jitter_buffer.pop_item(current_tick) {
            return Some((tick, index, payload));
        }
        return None;
    }
}
