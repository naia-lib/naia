
use crate::{EntityType, EntityKey, EntityStore, PacketReader, EntityManifest, NetEntity, LocalEntityKey, ClientEntityMessage, StateMask};
use std::{
    rc::Rc,
    cell::RefCell,
    collections::{VecDeque, HashMap}
};
use byteorder::{BigEndian, ReadBytesExt};
use log::warn;

pub struct ClientEntityManager<T: EntityType> {
    local_entity_store: HashMap<LocalEntityKey, T>,
    queued_incoming_messages: VecDeque<ClientEntityMessage<T>>,
}

impl<T: EntityType> ClientEntityManager<T> {
    pub fn new() -> Self {
        ClientEntityManager {
            queued_incoming_messages: VecDeque::new(),
            local_entity_store: HashMap::new(),
        }
    }

    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &EntityManifest<T>) {
        let buffer = reader.get_buffer();
        let cursor = reader.get_cursor();

        if let Ok(entity_message_count) = cursor.read_u8() {
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
                                new_entity.read(&entity_payload);
                                self.local_entity_store.insert(local_key, new_entity.clone()); //TODO, throw error if entity already exists using local key
                                self.queued_incoming_messages.push_back(ClientEntityMessage::Create(local_key, new_entity));
                            }
                            _ => {}
                        }
                    },
                    1 => { // Deletion
                        let local_key: LocalEntityKey = cursor.read_u16::<BigEndian>().unwrap().into();
                        self.local_entity_store.remove(&local_key);
                        self.queued_incoming_messages.push_back(ClientEntityMessage::Delete(local_key));
                    },
                    2 => { // Update
                        let local_key: LocalEntityKey = cursor.read_u16::<BigEndian>().unwrap().into();

                        if let Some(entity_ref) = self.local_entity_store.get_mut(&local_key) {
                            let state_mask: StateMask = StateMask::read(cursor);
                            let payload_length: u8 = cursor.read_u8().unwrap().into();
                            let payload_start_position: usize = cursor.position() as usize;
                            let payload_end_position: usize = payload_start_position + (payload_length as usize);

                            let entity_payload = buffer[payload_start_position..payload_end_position]
                                .to_vec()
                                .into_boxed_slice();

                            entity_ref.read_partial(&state_mask, &entity_payload);
                            self.queued_incoming_messages.push_back(ClientEntityMessage::Update(local_key));
                        }
                    },
                    _ => {}
                }
            }
        } else {
            warn!("Something up with server writing multiple messages here");
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ClientEntityMessage<T>> {
        return self.queued_incoming_messages.pop_front();
    }

    pub fn get_local_entity(&self, key: LocalEntityKey) -> Option<&T> {
        return self.local_entity_store.get(&key);
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