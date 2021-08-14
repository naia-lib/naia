use std::{collections::HashSet, net::SocketAddr};

use naia_shared::{
    State, StateType, Connection, ConnectionConfig, ManagerType, Manifest,
    PacketReader, PacketType, Ref, SequenceNumber, StandardHeader, EntityKey, PawnKey
};

use super::{
    state::{
        object_key::object_key::ObjectKey, mut_handler::MutHandler,
        server_state_manager::ServerStateManager,
    },
    command_receiver::CommandReceiver,
    ping_manager::PingManager,
    server_packet_writer::ServerPacketWriter,
};
use crate::{ComponentKey, GlobalPawnKey};

pub struct ClientConnection<U: StateType> {
    connection: Connection<U>,
    state_manager: ServerStateManager<U>,
    ping_manager: PingManager,
    command_receiver: CommandReceiver<U>,
}

impl<U: StateType> ClientConnection<U> {
    pub fn new(
        address: SocketAddr,
        mut_handler: Option<&Ref<MutHandler>>,
        connection_config: &ConnectionConfig,
    ) -> Self {
        ClientConnection {
            connection: Connection::new(address, connection_config),
            state_manager: ServerStateManager::new(address, mut_handler.unwrap()),
            ping_manager: PingManager::new(),
            command_receiver: CommandReceiver::new(),
        }
    }

    pub fn get_outgoing_packet(
        &mut self,
        host_tick: u16,
        manifest: &Manifest<U>,
    ) -> Option<Box<[u8]>> {
        if self.connection.has_outgoing_events() || self.state_manager.has_outgoing_messages() {
            let mut writer = ServerPacketWriter::new();

            let next_packet_index: u16 = self.get_next_packet_index();
            while let Some(popped_event) = self.connection.pop_outgoing_event(next_packet_index) {
                if !writer.write_event(manifest, &popped_event) {
                    self.connection
                        .unpop_outgoing_event(next_packet_index, &popped_event);
                    break;
                }
            }
            while let Some(popped_state_message) =
                self.state_manager.pop_outgoing_message(next_packet_index)
            {
                if !self.state_manager.write_state_message(
                    &mut writer,
                    manifest,
                    &popped_state_message,
                ) {
                    self.state_manager
                        .unpop_outgoing_message(next_packet_index, &popped_state_message);
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
                ManagerType::Event => {
                    self.connection.process_event_data(&mut reader, manifest);
                }
                _ => {}
            }
        }
    }

    pub fn has_state(&self, key: &ObjectKey) -> bool {
        return self.state_manager.has_state(key);
    }

    pub fn add_state(&mut self, key: &ObjectKey, state: &Ref<dyn State<U>>) {
        self.state_manager.add_state(key, state);
    }

    pub fn remove_state(&mut self, key: &ObjectKey) {
        self.state_manager.remove_state(key);
    }

    pub fn collect_state_updates(&mut self) {
        self.state_manager.collect_state_updates();
    }

    pub fn has_pawn(&self, key: &ObjectKey) -> bool {
        return self.state_manager.has_pawn(key);
    }

    pub fn add_pawn(&mut self, key: &ObjectKey) {
        self.state_manager.add_pawn(key);
    }

    pub fn remove_pawn(&mut self, key: &ObjectKey) {
        self.state_manager.remove_pawn(key);
    }

    pub fn get_incoming_command(&mut self, server_tick: u16) -> Option<(GlobalPawnKey, U)> {
        if let Some((local_pawn_key, command)) =
            self.command_receiver.pop_incoming_command(server_tick)
        {
            match local_pawn_key {
                PawnKey::State(local_object_key) => {
                    if let Some(global_pawn_key) =
                        self.state_manager.get_global_key_from_local(local_object_key)
                    {
                        return Some((GlobalPawnKey::State(*global_pawn_key), command));
                    }
                }
                PawnKey::Entity(local_entity_key) => {
                    if let Some(global_pawn_key) =
                        self.state_manager.get_global_entity_key_from_local(local_entity_key)
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
        return self.state_manager.has_entity(key);
    }

    pub fn add_entity(&mut self, key: &EntityKey, components_ref: &Ref<HashSet<ComponentKey>>, component_list: &Vec<(ComponentKey, Ref<dyn State<U>>)>) {
        self.state_manager.add_entity(key, components_ref, component_list);
    }

    pub fn remove_entity(&mut self, key: &EntityKey) {
        self.state_manager.remove_entity(key);
    }

    pub fn has_pawn_entity(&self, key: &EntityKey) -> bool {
        return self.state_manager.has_pawn_entity(key);
    }

    pub fn add_pawn_entity(&mut self, key: &EntityKey) {
        self.state_manager.add_pawn_entity(key);
    }

    pub fn remove_pawn_entity(&mut self, key: &EntityKey) {
        self.state_manager.remove_pawn_entity(key);
    }

    pub fn add_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey, component_ref: &Ref<dyn State<U>>) {
        self.state_manager.add_component(entity_key, component_key, component_ref);
    }

//    pub fn remove_component(&mut self, entity_key: &EntityKey, component_key: &ComponentKey) {
//        self.state_manager.remove_component(entity_key, component_key);
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
            .process_incoming_header(header, &mut Some(&mut self.state_manager));
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

    pub fn queue_event(&mut self, event: &impl State<U>) {
        return self.connection.queue_event(event);
    }

    pub fn get_incoming_event(&mut self) -> Option<U> {
        return self.connection.get_incoming_event();
    }

    pub fn get_address(&self) -> SocketAddr {
        return self.connection.get_address();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        return self.connection.get_last_received_tick();
    }
}
