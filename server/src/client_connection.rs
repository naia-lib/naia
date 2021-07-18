use std::net::SocketAddr;

use naia_shared::{
    Actor, ActorType, Connection, ConnectionConfig, Event, EventType, ManagerType, Manifest,
    PacketReader, PacketType, Ref, SequenceNumber, StandardHeader, LocalActorKey
};

use super::{
    actors::{
        actor_key::actor_key::ActorKey, actor_packet_writer::ActorPacketWriter,
        mut_handler::MutHandler, server_actor_manager::ServerActorManager,
    },
    command_receiver::CommandReceiver,
    ping_manager::PingManager,
    server_packet_writer::ServerPacketWriter,
};

pub struct ClientConnection<T: EventType, U: ActorType> {
    connection: Connection<T>,
    actor_manager: ServerActorManager<U>,
    ping_manager: PingManager,
    command_receiver: CommandReceiver<T>,
}

impl<T: EventType, U: ActorType> ClientConnection<T, U> {
    pub fn new(
        address: SocketAddr,
        mut_handler: Option<&Ref<MutHandler>>,
        connection_config: &ConnectionConfig,
    ) -> Self {
        ClientConnection {
            connection: Connection::new(address, connection_config),
            actor_manager: ServerActorManager::new(address, mut_handler.unwrap()),
            ping_manager: PingManager::new(),
            command_receiver: CommandReceiver::new(),
        }
    }

    pub fn get_outgoing_packet(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<T, U>,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_events() || self.actor_manager.has_outgoing_messages() {
            let mut writer = ServerPacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.connection.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(manifest, &popped_event) {
                    self.connection
                        .unpop_outgoing_event(next_packet_index, &popped_event);
                    break;
                }
            }
            while let Some(popped_actor_message) =
                self.actor_manager.pop_outgoing_message(next_packet_index)
            {
                if !ActorPacketWriter::write_actor_message(
                    &mut writer,
                    manifest,
                    &popped_actor_message,
                ) {
                    self.actor_manager
                        .unpop_outgoing_message(next_packet_index, &popped_actor_message);
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
        server_tick: u16,
        client_tick: u16,
        manifest: &Manifest<T, U>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Command => {
                    self.command_receiver.process_data(
                        server_tick,
                        client_tick,
                        &mut reader,
                        manifest,
                    );
                }
                ManagerType::Event => {
                    self.connection.process_event_data(&mut reader, manifest);
                }
                _ => {}
            }
        }
    }

    pub fn has_actor(&self, key: &ActorKey) -> bool {
        return self.actor_manager.has_actor(key);
    }

    pub fn add_actor(&mut self, key: &ActorKey, actor: &Ref<dyn Actor<U>>) {
        self.actor_manager.add_actor(key, actor);
    }

    pub fn remove_actor(&mut self, key: &ActorKey) {
        self.actor_manager.remove_actor(key);
    }

    pub fn collect_actor_updates(&mut self) {
        self.actor_manager.collect_actor_updates();
    }

    pub fn has_pawn(&self, key: &ActorKey) -> bool {
        return self.actor_manager.has_pawn(key);
    }

    pub fn add_pawn(&mut self, key: &ActorKey) {
        self.actor_manager.add_pawn(key);
    }

    pub fn remove_pawn(&mut self, key: &ActorKey) {
        self.actor_manager.remove_pawn(key);
    }

    pub fn get_actor_local_key(&self, key: &ActorKey) -> Option<LocalActorKey> {
        return self.actor_manager.get_local_key_from_global(key);
    }

    pub fn actor_is_created(&self, local_key: &LocalActorKey) -> bool {
        return self.actor_manager.actor_is_created(local_key);
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

    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        self.connection
            .process_incoming_header(header, &mut Some(&mut self.actor_manager));
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

    pub fn get_incoming_command(&mut self, server_tick: u16) -> Option<(ActorKey, T)> {
        if let Some((local_pawn_key, command)) =
            self.command_receiver.pop_incoming_command(server_tick)
        {
            if let Some(global_pawn_key) =
                self.actor_manager.get_global_key_from_local(local_pawn_key)
            {
                return Some((*global_pawn_key, command));
            }
        }
        return None;
    }

    pub fn get_address(&self) -> SocketAddr {
        return self.connection.get_address();
    }

    pub fn process_ping(&self, ping_payload: &[u8]) -> Box<[u8]> {
        return self.ping_manager.process_ping(ping_payload);
    }

    pub fn get_last_received_tick(&self) -> u16 {
        return self.connection.get_last_received_tick();
    }
}
