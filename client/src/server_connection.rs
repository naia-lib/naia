use std::{collections::hash_map::Iter, net::SocketAddr, rc::Rc};

use naia_shared::{
    Connection, ConnectionConfig, EntityType, Event, EventType, LocalEntityKey, ManagerType,
    Manifest, PacketReader, PacketType, PacketWriter, SequenceNumber, StandardHeader,
};

use super::{
    client_entity_manager::ClientEntityManager, client_entity_message::ClientEntityMessage,
    command_sender::CommandSender, ping_manager::PingManager,
};
use crate::{client_tick_manager::ClientTickManager, command_receiver::CommandReceiver, Packet};

#[derive(Debug)]
pub struct ServerConnection<T: EventType, U: EntityType> {
    connection: Connection<T>,
    entity_manager: ClientEntityManager<U>,
    ping_manager: PingManager,
    command_sender: CommandSender<T>,
    command_receiver: CommandReceiver<T>,
}

impl<T: EventType, U: EntityType> ServerConnection<T, U> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            entity_manager: ClientEntityManager::new(),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.rtt_sample_size,
            ),
            command_sender: CommandSender::new(),
            command_receiver: CommandReceiver::new(),
        };
    }

    pub fn get_outgoing_packet(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<T, U>,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_events() || self.command_sender.has_command() {
            let mut writer = PacketWriter::new();

            while let Some((pawn_key, command)) = self.command_sender.pop_command() {
                if !writer.write_command(manifest, pawn_key, &command) {
                    self.command_sender.unpop_command(pawn_key, &command);
                    break;
                } else {
                    self.command_receiver
                        .queue_command(host_tick, pawn_key, &command);
                }
            }

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.connection.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(manifest, &popped_event) {
                    self.connection
                        .unpop_outgoing_event(next_packet_index, &popped_event);
                    break;
                }
            }

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

    pub fn process_incoming_data(
        &mut self,
        packet_tick: u16,
        packet_index: u16,
        manifest: &Manifest<T, U>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Event => {
                    self.connection.process_event_data(&mut reader, manifest);
                }
                ManagerType::Entity => {
                    self.entity_manager.process_data(
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

    // Pass-through methods to underlying entity manager
    pub fn get_incoming_entity_message(&mut self) -> Option<ClientEntityMessage> {
        return self.entity_manager.pop_incoming_message();
    }

    pub fn entities_iter(&self) -> Iter<LocalEntityKey, U> {
        return self.entity_manager.entities_iter();
    }

    pub fn get_local_entity(&self, key: LocalEntityKey) -> Option<&U> {
        return self.entity_manager.get_local_entity(key);
    }

    pub fn pawns_iter(&self) -> Iter<LocalEntityKey, U> {
        return self.entity_manager.pawns_iter();
    }

    pub fn get_pawn(&self, key: LocalEntityKey) -> Option<&U> {
        return self.entity_manager.get_pawn(key);
    }

    pub fn save_pawn_snapshots(&mut self, tick: u16) {
        return self.entity_manager.save_pawn_snapshots(tick);
    }

    pub fn fill_history(&self, buffer: &mut Vec<U>) {
        self.entity_manager.fill_history(buffer);
    }

    // Pass-through methods to underlying common connection

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
        tick_manager: &mut ClientTickManager,
    ) {
        tick_manager.project_intended_tick(
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

    pub fn queue_event(&mut self, event: &impl Event<T>) {
        return self.connection.queue_event(event);
    }

    pub fn get_incoming_event(&mut self) -> Option<T> {
        return self.connection.get_incoming_event();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        self.connection.get_last_received_tick()
    }

    // command related
    pub fn queue_command(&mut self, pawn_key: LocalEntityKey, command: &impl Event<T>) {
        return self.command_sender.queue_command(pawn_key, command);
    }

    pub fn get_incoming_command(&mut self) -> Option<(LocalEntityKey, Rc<Box<dyn Event<T>>>)> {
        if let Some((history_tick, pawn_key, command)) = self
            .command_receiver
            .pop_command_replay::<U>(&mut self.entity_manager)
        {
            self.entity_manager
                .save_replay_snapshot(history_tick - 1, &pawn_key);
            return Some((pawn_key, command));
        }
        return self.command_receiver.pop_command();
    }

    // ping related
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
}
