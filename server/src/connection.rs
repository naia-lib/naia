use std::{
    collections::HashSet,
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType,
    ProtocolType, ReplicateSafe, SequenceNumber, StandardHeader, WorldRefType,
};

use super::{
    command_receiver::CommandReceiver, entity_manager::EntityManager,
    global_diff_handler::GlobalDiffHandler, keys::ComponentKey, packet_writer::PacketWriter,
    ping_manager::PingManager, user::user_key::UserKey, world_record::WorldRecord,
};

pub struct Connection<P: ProtocolType, E: Copy + Eq + Hash> {
    pub user_key: UserKey,
    owned_entities: HashSet<E>,
    base_connection: BaseConnection<P>,
    entity_manager: EntityManager<P, E>,
    ping_manager: PingManager,
    command_receiver: CommandReceiver<P>,
}

impl<P: ProtocolType, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        user_address: SocketAddr,
        user_key: &UserKey,
        diff_handler: &Arc<RwLock<GlobalDiffHandler>>,
    ) -> Self {
        Connection {
            user_key: *user_key,
            owned_entities: HashSet::new(),
            base_connection: BaseConnection::new(user_address, connection_config),
            entity_manager: EntityManager::new(user_address, diff_handler),
            ping_manager: PingManager::new(),
            command_receiver: CommandReceiver::new(),
        }
    }

    pub fn get_outgoing_packet<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, P::Kind>,
        host_tick: Option<u16>,
    ) -> Option<Box<[u8]>> {
        if self.base_connection.has_outgoing_messages()
            || self.entity_manager.has_outgoing_actions()
        {
            let mut writer = PacketWriter::new();

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
            while let Some(popped_entity_action) = self
                .entity_manager
                .pop_outgoing_action::<W>(world_record, next_packet_index)
            {
                if !self.entity_manager.write_entity_action(
                    world,
                    &mut writer,
                    &popped_entity_action,
                ) {
                    self.entity_manager
                        .unpop_outgoing_action(next_packet_index, popped_entity_action);
                    break;
                }
            }

            if writer.has_bytes() {
                // Get bytes from writer
                let out_bytes = writer.get_bytes();

                // Add header to it
                let payload = self.process_outgoing_header(
                    host_tick,
                    self.base_connection.get_last_received_tick(),
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
        server_tick: Option<u16>,
        client_tick: u16,
        manifest: &Manifest<P>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::Command => {
                    self.command_receiver.process_incoming_commands(
                        server_tick,
                        client_tick,
                        &mut reader,
                        manifest,
                    );
                }
                ManagerType::Message => {
                    // packet index shouldn't matter here because the server's impl of Property
                    // doesn't use it
                    self.base_connection
                        .process_message_data(&mut reader, manifest, 0);
                }
                _ => {}
            }
        }
    }

    pub fn collect_component_updates(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        self.entity_manager.collect_component_updates(world_record);
    }

    pub fn get_incoming_command(&mut self, server_tick: u16) -> Option<(E, P)> {
        if let Some((local_entity, command)) =
            self.command_receiver.pop_incoming_command(server_tick)
        {
            // get global entity from the local one
            if let Some(global_entity) = self
                .entity_manager
                .get_global_entity_from_local(local_entity)
            {
                // make sure Command is valid (the entity really is owned by this connection)
                if self.entity_manager.has_entity_prediction(global_entity) {
                    return Some((*global_entity, command));
                }
            }
        }
        return None;
    }

    pub fn process_ping(&self, ping_payload: &[u8]) -> Box<[u8]> {
        return self.ping_manager.process_ping(ping_payload);
    }

    // Entity management

    pub fn has_entity(&self, entity: &E) -> bool {
        return self.entity_manager.has_entity(entity);
    }

    pub fn spawn_entity(&mut self, world_record: &WorldRecord<E, P::Kind>, entity: &E) {
        self.entity_manager.spawn_entity(world_record, entity);
    }

    pub fn despawn_entity(&mut self, world_record: &WorldRecord<E, P::Kind>, entity: &E) {
        self.entity_manager.despawn_entity(world_record, entity);
    }

    pub fn has_prediction_entity(&self, entity: &E) -> bool {
        return self.entity_manager.has_entity_prediction(entity);
    }

    pub fn add_prediction_entity(&mut self, entity: &E) {
        self.entity_manager.add_prediction_entity(entity);
    }

    pub fn remove_prediction_entity(&mut self, entity: &E) {
        self.entity_manager.remove_prediction_entity(entity);
    }

    pub fn insert_component(
        &mut self,
        world_record: &WorldRecord<E, P::Kind>,
        component_key: &ComponentKey,
    ) {
        self.entity_manager
            .insert_component(world_record, component_key);
    }

    pub fn remove_component(&mut self, component_key: &ComponentKey) {
        self.entity_manager.remove_component(component_key);
    }

    // Pass-through methods to underlying common connection

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
        world_record: &WorldRecord<E, P::Kind>,
        header: &StandardHeader,
    ) {
        self.base_connection
            .process_incoming_header(header, &mut Some(&mut self.entity_manager));
        self.entity_manager.process_delivered_packets(world_record);
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

    pub fn get_incoming_message(&mut self) -> Option<P> {
        return self.base_connection.get_incoming_message();
    }

    pub fn address(&self) -> SocketAddr {
        return self.base_connection.get_address();
    }

    pub fn get_last_received_tick(&self) -> u16 {
        return self.base_connection.get_last_received_tick();
    }

    pub fn own_entity(&mut self, entity: &E) {
        self.owned_entities.insert(*entity);
    }

    pub fn disown_entity(&mut self, entity: &E) {
        self.owned_entities.remove(&entity);
    }

    pub fn owned_entities(&self) -> Vec<E> {
        let mut output = Vec::new();

        for owned_entity in &self.owned_entities {
            output.push(*owned_entity);
        }

        return output;
    }
}
