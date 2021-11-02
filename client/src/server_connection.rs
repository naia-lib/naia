use std::{collections::VecDeque, hash::Hash, net::SocketAddr};

use naia_client_socket::Packet;

use naia_shared::{
    Connection, ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolType,
    ReplicateSafe, SequenceNumber, StandardHeader, WorldMutType,
};

use super::{
    command_receiver::CommandReceiver, entity_action::EntityAction, entity_manager::EntityManager,
    owned_entity::OwnedEntity, packet_writer::PacketWriter, ping_manager::PingManager,
    tick_manager::TickManager, tick_queue::TickQueue,
};

pub struct ServerConnection<P: ProtocolType, E: Copy + Eq + Hash> {
    connection: Connection<P>,
    entity_manager: EntityManager<P, E>,
    ping_manager: PingManager,
    command_sender: VecDeque<(OwnedEntity<E>, P)>,
    command_receiver: CommandReceiver<P, E>,
    jitter_buffer: TickQueue<(u16, Box<[u8]>)>,
}

impl<P: ProtocolType, E: Copy + Eq + Hash> ServerConnection<P, E> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            entity_manager: EntityManager::new(),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.rtt_sample_size,
            ),
            command_sender: VecDeque::new(),
            command_receiver: CommandReceiver::new(),
            jitter_buffer: TickQueue::new(),
        };
    }

    pub fn get_outgoing_packet(&mut self, host_tick: u16) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_messages() || !self.command_sender.is_empty() {
            let mut writer = PacketWriter::new();

            // Commands
            while let Some((owned_entity, command)) = self.command_sender.pop_front() {
                if writer.write_command(
                    host_tick,
                    &self.entity_manager,
                    &self.command_receiver,
                    &owned_entity,
                    &command,
                ) {
                    self.command_receiver
                        .queue_command(host_tick, owned_entity, command);
                } else {
                    self.command_sender.push_front((owned_entity, command));
                    break;
                }
            }

            // Messages
            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_message) = self.connection.pop_outgoing_message(next_packet_index)
            {
                if !writer.write_message(&popped_message) {
                    self.connection
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
                    host_tick,
                    self.connection.get_last_received_tick(),
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
        packet_tick: u16,
        packet_index: u16,
        manifest: &Manifest<P>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Message => {
                    self.connection
                        .process_message_data(&mut reader, manifest, packet_index);
                }
                ManagerType::Entity => {
                    self.entity_manager.process_data(
                        world,
                        manifest,
                        &mut self.command_receiver,
                        packet_tick,
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

    pub fn entity_is_owned(&self, key: &E) -> bool {
        return self.entity_manager.entity_is_owned(key);
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
            let target_tick = tick_manager.get_server_tick();
            while let Some((tick, packet_index, data_packet)) =
                self.get_buffered_data_packet(target_tick)
            {
                self.process_incoming_data(world, tick, packet_index, manifest, &data_packet);
            }
            return true;
        }
        return false;
    }

    // Pass-through methods to underlying Connection

    pub fn mark_sent(&mut self) {
        return self.connection.mark_sent();
    }

    pub fn should_send_heartbeat(&self) -> bool {
        return self.connection.should_send_heartbeat();
    }

    pub fn mark_heard(&mut self) {
        return self.connection.mark_heard();
    }

    pub fn should_drop(&self) -> bool {
        return self.connection.should_drop();
    }

    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        tick_manager: &mut TickManager,
    ) {
        tick_manager.record_server_tick(
            header.host_tick(),
            self.ping_manager.get_rtt(),
            self.ping_manager.get_jitter(),
        );
        self.connection.process_incoming_header(header, &mut None);
    }

    pub fn process_outgoing_header(
        &mut self,
        host_tick: u16,
        last_received_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self.connection.process_outgoing_header(
            host_tick,
            last_received_tick,
            packet_type,
            payload,
        );
    }

    pub fn get_next_packet_index(&self) -> SequenceNumber {
        return self.connection.get_next_packet_index();
    }

    pub fn queue_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        return self.connection.queue_message(message, guaranteed_delivery);
    }

    pub fn get_incoming_message(&mut self) -> Option<P> {
        return self.connection.get_incoming_message();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        self.connection.get_last_received_tick()
    }

    // Commands
    pub fn queue_command<R: ReplicateSafe<P>>(&mut self, entity: OwnedEntity<E>, command: R) {
        let command_protocol = command.into_protocol();
        return self.command_sender.push_back((entity, command_protocol));
    }

    pub fn process_replays<W: WorldMutType<P, E>>(&mut self, world: &mut W) {
        self.command_receiver
            .process_command_replay(world, &mut self.entity_manager);
    }

    pub fn get_incoming_replay(&mut self) -> Option<(OwnedEntity<E>, P)> {
        if let Some((_tick, owned_entity, command)) = self.command_receiver.pop_command_replay() {
            return Some((owned_entity, command));
        }

        return None;
    }

    pub fn get_incoming_command(&mut self) -> Option<(OwnedEntity<E>, P)> {
        if let Some((_tick, owned_entity, command)) = self.command_receiver.pop_command() {
            return Some((owned_entity, command));
        }
        return None;
    }

    pub fn get_confirmed_entity(&self, predicted_entity: &E) -> Option<&E> {
        return self.entity_manager.get_confirmed_entity(predicted_entity);
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

    fn get_buffered_data_packet(&mut self, current_tick: u16) -> Option<(u16, u16, Box<[u8]>)> {
        if let Some((tick, (index, payload))) = self.jitter_buffer.pop_item(current_tick) {
            return Some((tick, index, payload));
        }
        return None;
    }
}
