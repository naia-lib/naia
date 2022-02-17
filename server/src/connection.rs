use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, Protocolize,
    ReplicateSafe, StandardHeader, WorldRefType,
};

use super::{
    entity_manager::EntityManager, entity_message_receiver::EntityMessageReceiver,
    global_diff_handler::GlobalDiffHandler, keys::ComponentKey, packet_writer::PacketWriter,
    ping_manager::PingManager, user::user_key::UserKey, world_record::WorldRecord,
};

pub type PacketIndex = u16;

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    pub user_key: UserKey,
    base_connection: BaseConnection<P>,
    entity_manager: EntityManager<P, E>,
    ping_manager: PingManager,
    entity_message_receiver: EntityMessageReceiver<P>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> Connection<P, E> {
    pub fn new(
        connection_config: &ConnectionConfig,
        user_address: SocketAddr,
        user_key: &UserKey,
        diff_handler: &Arc<RwLock<GlobalDiffHandler>>,
    ) -> Self {
        Connection {
            user_key: *user_key,
            base_connection: BaseConnection::new(user_address, connection_config),
            entity_manager: EntityManager::new(user_address, diff_handler),
            ping_manager: PingManager::new(),
            entity_message_receiver: EntityMessageReceiver::new(),
        }
    }

    pub fn outgoing_packet<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, P::Kind>,
        server_tick: u16,
    ) -> Option<Box<[u8]>> {
        if self.base_connection.has_outgoing_messages()
            || self.entity_manager.has_outgoing_actions()
        {
            let mut writer = PacketWriter::new();

            let next_packet_index: PacketIndex = self.next_packet_index();

            // Write Messages
            loop {
                if let Some(peeked_message) = self.base_connection.peek_outgoing_message() {
                    if !writer.message_fits(peeked_message) {
                        break;
                    }
                } else {
                    break;
                }

                let popped_message = self.base_connection.pop_outgoing_message(next_packet_index).unwrap();
                writer.write_message(&popped_message);
            }

            // Write Entity actions
            loop {


                if !self.entity_manager.peek_action_fits::<W>(world_record, &writer) {
                    break;
                }

                let popped_entity_action = self
                    .entity_manager
                    .pop_outgoing_action::<W>(world_record, next_packet_index).unwrap();
                self.entity_manager.write_entity_action(world,
                                                        &mut writer,
                                                        &popped_entity_action);
            }

            // while let Some(popped_entity_action) = self
            //     .entity_manager
            //     .pop_outgoing_action::<W>(world_record, next_packet_index)
            // {
            //     if !self.entity_manager.write_entity_action(
            //         world,
            //         &mut writer,
            //         &popped_entity_action,
            //     ) {
            //         self.entity_manager
            //             .unpop_outgoing_action(next_packet_index, popped_entity_action);
            //         break;
            //     }
            // }

            if writer.has_bytes() {
                // Get bytes from writer
                let out_bytes = writer.bytes();

                // Add header to it
                let payload =
                    self.process_outgoing_header(server_tick, PacketType::Data, &out_bytes);
                return Some(payload);
            }
        }

        return None;
    }

    pub fn process_incoming_data(
        &mut self,
        server_tick: Option<u16>,
        manifest: &Manifest<P>,
        data: &[u8],
    ) {
        let mut reader = PacketReader::new(data);
        while reader.has_more() {
            let manager_type: ManagerType = reader.read_u8().into();
            match manager_type {
                ManagerType::EntityMessage => {
                    self.entity_message_receiver.process_incoming_messages(
                        server_tick,
                        &mut reader,
                        manifest,
                    );
                }
                ManagerType::Message => {
                    // packet index shouldn't matter here because the server's impl of Property
                    // doesn't use it
                    self.base_connection
                        .process_message_data(&mut reader, manifest);
                }
                _ => {}
            }
        }
    }

    pub fn collect_component_updates(&mut self, world_record: &WorldRecord<E, P::Kind>) {
        self.entity_manager.collect_component_updates(world_record);
    }

    pub fn pop_incoming_entity_message(&mut self, server_tick: u16) -> Option<(E, P)> {
        if let Some((local_entity, message)) = self
            .entity_message_receiver
            .pop_incoming_entity_message(server_tick)
        {
            // get global entity from the local one
            if let Some(global_entity) = self.entity_manager.global_entity_from_local(local_entity)
            {
                return Some((*global_entity, message));
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

    pub fn send_entity_message<R: ReplicateSafe<P>>(&mut self, entity: &E, message: &R) {
        self.entity_manager.send_entity_message(entity, message);
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
        host_tick: u16,
        packet_type: PacketType,
        payload: &[u8],
    ) -> Box<[u8]> {
        return self
            .base_connection
            .process_outgoing_header(host_tick, packet_type, payload);
    }

    pub fn next_packet_index(&self) -> PacketIndex {
        return self.base_connection.next_packet_index();
    }

    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        return self
            .base_connection
            .send_message(message, guaranteed_delivery);
    }

    pub fn incoming_message(&mut self) -> Option<P> {
        return self.base_connection.incoming_message();
    }

    pub fn address(&self) -> SocketAddr {
        return self.base_connection.address();
    }

    pub fn last_received_tick(&self) -> u16 {
        return self.base_connection.last_received_tick();
    }
}
