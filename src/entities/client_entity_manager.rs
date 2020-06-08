
use crate::{EntityType, EntityKey, EntityStore, PacketReader, EntityManifest, NetEntity, LocalEntityKey};
use std::{
    collections::VecDeque};
use byteorder::{BigEndian, ReadBytesExt};

pub struct ClientEntityManager<T: EntityType> {
    local_entity_store: EntityStore<T>,
}

impl<T: EntityType> ClientEntityManager<T> {
    pub fn new() -> Self {
        ClientEntityManager {
            local_entity_store:  EntityStore::new(),
        }
    }

    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
    }

    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
    }

    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &EntityManifest<T>) {
        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        let entity_message_count: u8 = cursor.read_u8().unwrap().into();
        for _x in 0..entity_message_count {
            let message_type: u8 = cursor.read_u8().unwrap().into();
            match message_type {
                0 => { // Creation
                    let gaia_id: u16 = cursor.read_u16::<BigEndian>().unwrap().into();
                    let local_key: LocalEntityKey = cursor.read_u16::<BigEndian>().unwrap().into();
                    let payload_length: u8 = cursor.read_u8().unwrap().into();
                    let payload_start_position: usize = cursor.position() as usize;
                    let payload_end_position: usize = payload_start_position + (payload_length as usize);

                    let entity_payload = buffer[payload_start_position..payload_end_position]
                        .to_vec()
                        .into_boxed_slice();

                    match manifest.create_entity(gaia_id) {
                        Some(mut new_entity) => {
                            //TODO: what do we do now?!?!
                            zzz
                            new_entity.read(&entity_payload);
                            self.queued_incoming_events.push_back(new_entity);
                        }
                        _ => {}
                    }
                },
                1 => { // Update
                },
                2 => { // Deletion
                    let local_key: LocalEntityKey = cursor.read_u16::<BigEndian>().unwrap().into();
                    //TODO: what do we do now?!?!
                    zzz
                },
            }
        }
    }

//    pub fn has_entity(&self, key: EntityKey) -> bool {
//        return self.local_entity_store.has_entity(key);
//    }
//
//    pub fn add_entity(&self, key: EntityKey) {
//        //return self.local_entity_store.has_entity(key);
//    }
//
//    pub fn remove_entity(&self, key: EntityKey) {
//        //return self.local_entity_store.has_entity(key);
//    }
}