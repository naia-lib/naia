use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

use naia_shared::{
    BaseConnection, ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType,
    PacketWriteState, Protocolize, StandardHeader, Tick, WorldRefType,
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
                    self.base
                        .message_manager
                        .process_message_data(&mut reader, manifest);
                }
                _ => {}
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
    ) -> Option<Box<[u8]>> {
        if self.base.message_manager.has_outgoing_messages()
            || self.entity_manager.has_outgoing_actions()
        {
            let mut write_state = PacketWriteState::new(self.base.next_packet_index());

            // Queue Messages for Write
            self.base.message_manager.queue_writes(&mut write_state);

            // Queue Entity Actions for Write
            self.entity_manager
                .queue_writes(&mut write_state, world, world_record);

            if write_state.byte_count() > 0 {
                // Get bytes from writer
                let mut out_vec = Vec::<u8>::new();
                self.base.message_manager.flush_writes(&mut out_vec);
                self.entity_manager.flush_writes(&mut out_vec);

                // Add header to it
                let payload = self.base.process_outgoing_header(
                    server_tick,
                    PacketType::Data,
                    &out_vec.into_boxed_slice(),
                );

                return Some(payload);
            } else {
                panic!("Pending outgoing messages but no bytes were written... Likely trying to transmit a Component/Message larger than 576 bytes!");
            }
        }

        return None;
    }
}
