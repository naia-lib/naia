use std::{net::SocketAddr, rc::Rc};

use naia_shared::{
    Connection, ConnectionConfig, EntityType, Event, EventType, LocalEntityKey, ManagerType,
    Manifest, PacketReader, PacketType, PacketWriter, SequenceIterator, SequenceNumber,
    StandardHeader,
};

use super::{
    client_entity_manager::ClientEntityManager, client_entity_message::ClientEntityMessage,
    command_sender::CommandSender, interpolation_manager::InterpolationManager,
    ping_manager::PingManager, tick_queue::TickQueue,
};
use crate::{client_tick_manager::ClientTickManager, command_receiver::CommandReceiver, Packet};
use std::collections::hash_map::Keys;

#[derive(Debug)]
pub struct ServerConnection<T: EventType, U: EntityType> {
    connection: Connection<T>,
    entity_manager: ClientEntityManager<U>,
    ping_manager: PingManager,
    command_sender: CommandSender<T>,
    command_receiver: CommandReceiver<T>,
    last_replay_tick: Option<(u16, LocalEntityKey)>,
    interpolation_manager: InterpolationManager<U>,
    jitter_buffer: TickQueue<(u16, Box<[u8]>)>,
}

impl<T: EventType, U: EntityType> ServerConnection<T, U> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
        tick_manager: &ClientTickManager,
    ) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            entity_manager: ClientEntityManager::new(),
            interpolation_manager: InterpolationManager::new(&tick_manager.get_tick_interval()),
            ping_manager: PingManager::new(
                connection_config.ping_interval,
                connection_config.rtt_sample_size,
            ),
            command_sender: CommandSender::new(),
            command_receiver: CommandReceiver::new(),
            last_replay_tick: None,
            jitter_buffer: TickQueue::new(),
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
                if writer.write_command(manifest, pawn_key, &command) {
                    self.command_receiver
                        .queue_command(host_tick, pawn_key, &command);
                } else {
                    self.command_sender.unpop_command(pawn_key, &command);
                    break;
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
                        &mut self.interpolation_manager,
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

    // Pass-through methods to underlying entity manager
    pub fn get_incoming_entity_message(&mut self) -> Option<ClientEntityMessage> {
        return self.entity_manager.pop_incoming_message();
    }

    pub fn entity_keys(&self) -> Keys<LocalEntityKey, U> {
        return self.entity_manager.entity_keys();
    }

    pub fn get_entity(
        &mut self,
        tick_manager: &ClientTickManager,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some(interpolated_entity) =
            self.interpolation_manager
                .get_interpolation(tick_manager, &self.entity_manager, key)
        {
            return Some(interpolated_entity);
        }
        return self.entity_manager.get_entity(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalEntityKey, U> {
        return self.entity_manager.pawn_keys();
    }

    pub fn pawn_history_iter(&self, pawn_key: &LocalEntityKey) -> Option<SequenceIterator<U>> {
        return self.entity_manager.pawn_history_iter(pawn_key);
    }

    pub fn get_pawn(
        &mut self,
        tick_manager: &ClientTickManager,
        key: &LocalEntityKey,
    ) -> Option<&U> {
        if let Some(interpolated_pawn) = self
            .interpolation_manager
            .get_pawn_interpolation(tick_manager, key)
        {
            return Some(interpolated_pawn);
        }
        return self.entity_manager.get_pawn(key);
    }

    pub fn get_pawn_mut(&mut self, key: &LocalEntityKey) -> Option<&U> {
        return self.entity_manager.get_pawn(key);
    }

    // Pass-through methods to underlying interpolation manager

    /// This doesn't actually interpolate all entities, but rather it marks the
    /// current time & tick in order to later present interpolated entities
    /// correctly. Call this at the beginning of any frame
    pub fn frame_begin(&mut self, manifest: &Manifest<T, U>, tick_manager: &mut ClientTickManager) {
        if tick_manager.mark_frame() {
            // interpolation manager snapshots current state of all entities
            self.interpolation_manager
                .snapshot_entities(&self.entity_manager);

            // then we apply all received updates to entities at once
            let target_tick = tick_manager.get_server_tick();
            while let Some((tick, packet_index, data_packet)) =
                self.get_buffered_data_packet(target_tick)
            {
                self.process_incoming_data(tick, packet_index, manifest, &data_packet);
            }

            // finally, we must update pawns since they may have been reconciled
            self.interpolation_manager
                .update_pawns(&self.entity_manager);
        }
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
        if let Some((last_replay_tick, pawn_key)) = self.last_replay_tick {
            self.entity_manager
                .save_replay_snapshot(last_replay_tick.wrapping_add(1), &pawn_key);
            self.last_replay_tick = None;
        }

        if let Some((tick, pawn_key, command)) = self
            .command_receiver
            .pop_command_replay::<U>(&mut self.entity_manager)
        {
            self.last_replay_tick = Some((tick, pawn_key));
            return Some((pawn_key, command));
        }
        if let Some((tick, pawn_key, command)) = self.command_receiver.pop_command() {
            self.last_replay_tick = Some((tick, pawn_key));
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
