use std::{collections::HashSet, net::SocketAddr};

use naia_shared::{
    Replicate, ProtocolType, Connection, ConnectionConfig, ManagerType, Manifest,
    PacketReader, PacketType, Ref, SequenceNumber, StandardHeader, EntityKey, PawnKey
};

use super::{
    replicate::{
        object_key::object_key::ObjectKey, mut_handler::MutHandler,
        replicate_manager::ReplicateManager,
    },
    command_receiver::CommandReceiver,
    ping_manager::PingManager,
    packet_writer::PacketWriter,
};
use crate::{ComponentKey, GlobalPawnKey};

pub struct ClientConnection<U: ProtocolType> {
    connection: Connection<U>,
    replicate_manager: ReplicateManager<U>,
    ping_manager: PingManager,
    command_receiver: CommandReceiver<U>,
}

impl<U: ProtocolType> ClientConnection<U> {
    pub fn new(
        address: SocketAddr,
        mut_handler: Option<&Ref<MutHandler>>,
        connection_config: &ConnectionConfig,
    ) -> Self {
        ClientConnection {
            connection: Connection::new(address, connection_config),
            replicate_manager: ReplicateManager::new(address, mut_handler.unwrap()),
            ping_manager: PingManager::new(),
            command_receiver: CommandReceiver::new(),
        }
    }

    pub fn get_outgoing_packet(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<U>,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_messages() || self.replicate_manager.has_outgoing_actions() {
            let mut writer = PacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_message) = self.connection.pop_outgoing_message(next_packet_index) {
                if !writer.write_message(manifest, &popped_message) {
                    self.connection
                        .unpop_outgoing_message(next_packet_index, &popped_message);
                    break;
                }
            }
            while let Some(popped_replicate_action) =
                self.replicate_manager.pop_outgoing_action(next_packet_index)
            {
                if !self.replicate_manager.write_replicate_action(
                    &mut writer,
                    manifest,
                    &popped_replicate_action,
                ) {
                    self.replicate_manager
                        .unpop_outgoing_action(next_packet_index, &popped_replicate_action);
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
        manifest: &Manifest<U>,
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
                ManagerType::Message => {
                    self.connection.process_message_data(&mut reader, manifest);
                }
                _ => {}
            }
        }
    }

    pub fn has_object(&self, key: &ObjectKey) -> bool {
        return self.replicate_manager.has_object(key);
    }

    pub fn add_replicate(&mut self, key: &ObjectKey, replicate: &Ref<dyn Replicate<U>>) {
        self.replicate_manager.add_replicate(key, replicate);
    }

    pub fn remove_replicate(&mut self, key: &ObjectKey) {
        self.replicate_manager.remove_replicate(key);
    }

    pub fn collect_replicate_updates(&mut self) {
        self.replicate_manager.collect_replicate_updates();
    }

    pub fn has_pawn(&self, key: &ObjectKey) -> bool {
        return self.replicate_manager.has_pawn(key);
    }

    pub fn add_pawn(&mut self, key: &ObjectKey) {
        self.replicate_manager.add_pawn(key);
    }

    pub fn remove_pawn(&mut self, key: &ObjectKey) {
        self.replicate_manager.remove_pawn(key);
    }

    pub fn get_incoming_command(&mut self, server_tick: u16) -> Option<(GlobalPawnKey, U)> {
        if let Some((local_pawn_key, command)) =
            self.command_receiver.pop_incoming_command(server_tick)
        {
            match local_pawn_key {
                PawnKey::Replicate(local_object_key) => {
                    if let Some(global_pawn_key) =
                        self.replicate_manager.get_global_key_from_local(local_object_key)
                    {
                        return Some((GlobalPawnKey::Replicate(*global_pawn_key), command));
                    }
                }
                PawnKey::Entity(local_entity_key) => {
                    if let Some(global_pawn_key) =
                        self.replicate_manager.get_global_entity_key_from_local(local_entity_key)
                    {
                        return Some((GlobalPawnKey::Entity(*global_pawn_key), command));
                    }
                }
            }

        }
        return None;
    }

    pub fn process_ping(&self, ping_payload: &[u8]) -> Box<[u8]> {
        return self.ping_manager.process_ping(ping_payload);
    }

    // Entity management

    pub fn has_entity(&self, key: &EntityKey) -> bool {
        return self.replicate_manager.has_entity(key);
    }

    pub fn add_entity(&mut self, key: &EntityKey, components_ref: &Ref<HashSet<ComponentKey>>, component_list: &Vec<(ComponentKey, Ref<dyn Replicate<U>>)>) {
        self.replicate_manager.add_entity(key, components_ref, component_list);
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        self.replicate_manager.remove_entity(key);
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.replicate_manager.has_pawn_entity(key);
    }

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        self.replicate_manager.add_pawn_entity(key);
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        self.replicate_manager.remove_pawn_entity(key);
    }

    pub fn add_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey, component_ref: &Ref<dyn Replicate<U>>) {
        self.replicate_manager.add_component(entity_key, component_key, component_ref);
    }

//    pub fn remove_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey) {
//        self.replicate_manager.remove_component(entity_key, component_key);
//    }

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
            .process_incoming_header(header, &mut Some(&mut self.replicate_manager));
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

    pub fn queue_message(&mut self, message: &impl Replicate<U>, guaranteed_delivery: bool) {
        return self.connection.queue_message(message, guaranteed_delivery);
    }

    pub fn get_incoming_message(&mut self) -> Option<U> {
        return self.connection.get_incoming_message();
    }

    pub fn get_address(&self) -> SocketAddr {
        return self.connection.get_address();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        return self.connection.get_last_received_tick();
    }
}
