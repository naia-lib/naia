use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketType,
    Protocolize, StandardHeader, Tick, WorldRefType, serde::{Serde, BitReader, BitWriter}
};

use super::{
    entity_manager::EntityManager, entity_message_receiver::EntityMessageReceiver,
    global_diff_handler::GlobalDiffHandler, user::user_key::UserKey, world_record::WorldRecord,
};

pub struct Connection<P: Protocolize, E: Copy + Eq + Hash> {
    pub user_key: UserKey,
    pub base: BaseConnection<P>,
    pub entity_manager: EntityManager<P, E>,
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
            base: BaseConnection::new(user_address, connection_config),
            entity_manager: EntityManager::new(user_address, diff_handler),
            entity_message_receiver: EntityMessageReceiver::new(),
        }
    }

    // Incoming Data

    pub fn process_incoming_header(
        &mut self,
        world_record: &WorldRecord<E, P::Kind>,
        header: &StandardHeader,
    ) {
        self.base
            .process_incoming_header(header, &mut Some(&mut self.entity_manager));
        self.entity_manager.process_delivered_packets(world_record);
    }

    pub fn process_incoming_data(
        &mut self,
        server_tick: Option<Tick>,
        manifest: &Manifest<P>,
        reader: &mut BitReader,
    ) {
        while reader.has_more() {
            let manager_type = ManagerType::de(reader).unwrap();
            match manager_type {
                ManagerType::EntityMessage => {
                    self.entity_message_receiver.process_incoming_messages(
                        server_tick,
                        reader,
                        manifest,
                    );
                }
                ManagerType::Message => {
                    // packet index shouldn't matter here because the server's impl of Property
                    // doesn't use it
                    self.base
                        .message_manager
                        .process_message_data(reader, manifest);
                }
                ManagerType::Entity => {
                    panic!("not yet allowed!");
                }
            }
        }
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

    // Outgoing data

    pub fn outgoing_packet<W: WorldRefType<P, E>>(
        &mut self,
        world: &W,
        world_record: &WorldRecord<E, P::Kind>,
        server_tick: Tick,
    ) -> Option<BitWriter> {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.has_outgoing_actions()
        {
            let next_packet_index = self.base.next_packet_index();

            let mut writer = BitWriter::new();

            // Add header
            self.base.write_outgoing_header(
                server_tick,
                PacketType::Data,
                &mut writer,
            );

            // Write Messages
            self.base.message_manager.write_messages(&mut writer, next_packet_index);

            // Write Entity Actions
            self.entity_manager
                .write_actions(&mut writer, next_packet_index, world, world_record);

            return Some(writer);
        }

        return None;
    }
}
