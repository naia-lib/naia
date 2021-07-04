use std::{net::SocketAddr, rc::Rc};

use naia_shared::{
    ActorType, Connection, ConnectionConfig, Event, EventType, LocalActorKey, ManagerType,
    Manifest, PacketReader, PacketType, SequenceNumber, StandardHeader,
};

use super::{
    client_actor_manager::ClientActorManager, client_actor_message::ClientActorMessage,
    client_packet_writer::ClientPacketWriter, command_sender::CommandSender,
    ping_manager::PingManager, tick_queue::TickQueue,
};
use crate::{client_tick_manager::ClientTickManager, command_receiver::CommandReceiver, Packet};
use std::collections::hash_map::Keys;

#[derive(Debug)]
pub struct ServerConnection<T: EventType, U: ActorType> {
    connection: Connection<T>,
    actor_manager: ClientActorManager<U>,
    ping_manager: PingManager,
    command_sender: CommandSender<T>,
    command_receiver: CommandReceiver<T>,
    last_replay_tick: Option<(u16, LocalActorKey)>,
    jitter_buffer: TickQueue<(u16, Box<[u8]>)>,
}

impl<T: EventType, U: ActorType> ServerConnection<T, U> {
    pub fn new(
        address: SocketAddr,
        connection_config: &ConnectionConfig,
    ) -> Self {
        return ServerConnection {
            connection: Connection::new(address, connection_config),
            actor_manager: ClientActorManager::new(),
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
            let mut writer = ClientPacketWriter::new();

            while let Some((pawn_key, command)) = self.command_sender.pop_command() {
                if writer.write_command(
                    host_tick,
                    manifest,
                    &self.command_receiver,
                    pawn_key,
                    &command,
                ) {
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
                ManagerType::Actor => {
                    self.actor_manager.process_data(
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

    // Pass-through methods to underlying actor manager
    pub fn get_incoming_actor_message(&mut self) -> Option<ClientActorMessage> {
        return self.actor_manager.pop_incoming_message();
    }

    pub fn actor_keys(&self) -> Keys<LocalActorKey, U> {
        return self.actor_manager.actor_keys();
    }

    pub fn get_actor(
        &self,
        key: &LocalActorKey,
    ) -> Option<&U> {
        return self.actor_manager.get_actor(key);
    }

    pub fn pawn_keys(&self) -> Keys<LocalActorKey, U> {
        return self.actor_manager.pawn_keys();
    }

    pub fn get_pawn(
        &self,
        key: &LocalActorKey,
    ) -> Option<&U> {
        return self.actor_manager.get_pawn(key);
    }

    pub fn get_pawn_mut(&mut self, key: &LocalActorKey) -> Option<&U> {
        return self.actor_manager.get_pawn(key);
    }

    /// Reads buffered incoming data on the appropriate tick boundary
    pub fn frame_begin(&mut self, manifest: &Manifest<T, U>, tick_manager: &mut ClientTickManager) -> bool {
        if tick_manager.mark_frame() {
            // then we apply all received updates to actors at once
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
    pub fn queue_command(&mut self, pawn_key: LocalActorKey, command: &impl Event<T>) {
        return self.command_sender.queue_command(pawn_key, command);
    }

    pub fn process_replay(&mut self) {

        self
            .command_receiver
            .process_command_replay::<U>(&mut self.actor_manager);

    }

    pub fn get_incoming_replay(&mut self) -> Option<(LocalActorKey, Rc<Box<dyn Event<T>>>)> {
        if let Some((_, _)) = self.last_replay_tick {
            self.last_replay_tick = None;
        }

        if let Some((tick, pawn_key, command)) = self
            .command_receiver
            .pop_command_replay::<U>()
        {
            self.last_replay_tick = Some((tick, pawn_key));
            return Some((pawn_key, command));
        }

        return None;
    }

    pub fn get_incoming_command(&mut self) -> Option<(LocalActorKey, Rc<Box<dyn Event<T>>>)> {
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
