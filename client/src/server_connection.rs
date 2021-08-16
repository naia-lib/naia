use std::{collections::hash_map::Keys, net::SocketAddr};

use naia_shared::{
    Connection, ConnectionConfig, LocalComponentKey, LocalEntityKey, LocalObjectKey, ManagerType,
    Manifest, PacketReader, PacketType, PawnKey, ProtocolType, Ref, Replicate, SequenceNumber,
    StandardHeader,
};

use crate::Packet;

use super::{
    dual_command_receiver::DualCommandReceiver, dual_command_sender::DualCommandSender,
    packet_writer::PacketWriter, ping_manager::PingManager, replica_action::ReplicaAction,
    replica_manager::ReplicaManager, tick_manager::TickManager, tick_queue::TickQueue,
};

#[derive(Debug)]
pub struct ServerConnection<T: ProtocolType> {
    connection: Connection<T>,
    replica_manager: ReplicaManager<T>,
    ping_manager: PingManager,
    command_sender: DualCommandSender<T>,
    command_receiver: DualCommandReceiver<T>,
    jitter_buffer: TickQueue<(u16, Box<[u8]>)>,
}

impl<T: ProtocolType> ServerConnection<T> {
    pub fn new(address: SocketAddr, connection_config: &ConnectionConfig) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            replica_manager: ReplicaManager::new(),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.rtt_sample_size,
            ),
            command_sender: DualCommandSender::new(),
            command_receiver: DualCommandReceiver::new(),
            jitter_buffer: TickQueue::new(),
        };
    }

    pub fn get_outgoing_packet(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<T>,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_messages() || self.command_sender.has_command() {
            let mut writer = PacketWriter::new();

            // Commands
            while let Some((pawn_key, command)) = self.command_sender.pop_command() {
                if writer.write_command(
                    host_tick,
                    manifest,
                    &self.command_receiver,
                    &pawn_key,
                    &command,
                ) {
                    self.command_receiver
                        .queue_command(host_tick, &pawn_key, &command);
                } else {
                    self.command_sender.unpop_command(&pawn_key, &command);
                    break;
                }
            }

            // Messages
            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_message) = self.connection.pop_outgoing_message(next_packet_index)
            {
                if !writer.write_message(manifest, &popped_message) {
                    self.connection
                        .unpop_outgoing_message(next_packet_index, &popped_message);
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

    pub fn process_incoming_data(
        &mut self,
        packet_tick: u16,
        packet_index: u16,
        manifest: &Manifest<T>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Message => {
                    self.connection.process_message_data(&mut reader, manifest);
                }
                ManagerType::Replica => {
                    self.replica_manager.process_data(
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

    pub fn get_buffered_data_packet(&mut self, current_tick: u16) -> Option<(u16, u16, Box<[u8]>)> {
        if let Some((tick, (index, payload))) = self.jitter_buffer.pop_item(current_tick) {
            return Some((tick, index, payload));
        }
        return None;
    }

    // Pass-through methods to underlying replica manager
    pub fn get_incoming_replica_action(&mut self) -> Option<ReplicaAction<T>> {
        return self.replica_manager.pop_incoming_message();
    }

    pub fn object_keys(&self) -> Vec<LocalObjectKey> {
        return self.replica_manager.object_keys();
    }

    pub fn component_keys(&self) -> Vec<LocalComponentKey> {
        return self.replica_manager.component_keys();
    }

    pub fn get_object(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.replica_manager.get_object(key);
    }

    pub fn has_object(&self, key: &LocalObjectKey) -> bool {
        return self.replica_manager.has_object(key);
    }

    pub fn has_component(&self, key: &LocalComponentKey) -> bool {
        return self.has_object(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalObjectKey, T> {
        return self.replica_manager.pawn_keys();
    }

    pub fn get_pawn(&self, key: &LocalObjectKey) -> Option<&T> {
        return self.replica_manager.get_pawn(key);
    }

    pub fn get_pawn_mut(&mut self, key: &LocalObjectKey) -> Option<&T> {
        return self.replica_manager.get_pawn(key);
    }

    pub fn has_entity(&self, key: &LocalEntityKey) -> bool {
        return self.replica_manager.has_entity(key);
    }

    /// Reads buffered incoming data on the appropriate tick boundary
    pub fn frame_begin(&mut self, manifest: &Manifest<T>, tick_manager: &mut TickManager) -> bool {
        if tick_manager.mark_frame() {
            // then we apply all received updates to replicas at once
            let target_tick = tick_manager.get_server_tick();
            while let Some((tick, packet_index, data_packet)) =
                self.get_buffered_data_packet(target_tick)
            {
                self.process_incoming_data(tick, packet_index, manifest, &data_packet);
            }
            return true;
        }
        return false;
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

    pub fn queue_message(&mut self, message: &Ref<dyn Replicate<T>>, guaranteed_delivery: bool) {
        return self.connection.queue_message(message, guaranteed_delivery);
    }

    pub fn get_incoming_message(&mut self) -> Option<T> {
        return self.connection.get_incoming_message();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        self.connection.get_last_received_tick()
    }

    // command related
    pub fn object_queue_command(
        &mut self,
        object_key: &LocalObjectKey,
        command: &Ref<dyn Replicate<T>>,
    ) {
        let pawn_key = PawnKey::Object(*object_key);
        return self.command_sender.queue_command(&pawn_key, command);
    }

    pub fn entity_queue_command(
        &mut self,
        entity_key: &LocalEntityKey,
        command: &Ref<dyn Replicate<T>>,
    ) {
        let pawn_key = PawnKey::Entity(*entity_key);
        return self.command_sender.queue_command(&pawn_key, command);
    }

    pub fn process_replays(&mut self) {
        self.command_receiver
            .process_command_replay::<T>(&mut self.replica_manager);
    }

    pub fn get_incoming_replay(&mut self) -> Option<(PawnKey, Ref<dyn Replicate<T>>)> {
        if let Some((_tick, pawn_key, command)) = self.command_receiver.pop_command_replay::<T>() {
            return Some((pawn_key, command));
        }

        return None;
    }

    pub fn get_incoming_command(&mut self) -> Option<(PawnKey, Ref<dyn Replicate<T>>)> {
        if let Some((_tick, pawn_key, command)) = self.command_receiver.pop_command() {
            return Some((pawn_key, command));
        }
        return None;
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
