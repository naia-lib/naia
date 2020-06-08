
use crate::{EntityType, EntityKey, EntityStore, PacketReader, EntityManifest, NetEntity, LocalEntityKey, ClientEntityMessage};
use std::{
    rc::Rc,
    collections::{VecDeque, HashMap}
};
use byteorder::{BigEndian, ReadBytesExt};

pub struct ClientEntityManager<T: EntityType> {
    local_entity_store: HashMap<LocalEntityKey, Rc<T>>,
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
                            new_entity.read(&entity_payload);
                            let rc_new_entity = Rc::new(new_entity);
                            self.local_entity_store.insert(local_key, rc_new_entity.clone()); //TODO, throw error if entity already exists using local key
                            self.queued_incoming_messages.push_back(ClientEntityMessage::Create(local_key, rc_new_entity));
                        }
                        _ => {}
                    }
                },
                1 => { // Update
                },
                2 => { // Deletion
                    let local_key: LocalEntityKey = cursor.read_u16::<BigEndian>().unwrap().into();
                    self.local_entity_store.remove(&local_key);
                    self.queued_incoming_messages.push_back(ClientEntityMessage::Delete(local_key));
                },
                _ => {}
            }
        }
    }

    pub fn pop_incoming_message(&mut self) -> Option<ClientEntityMessage<T>> {
        return self.queued_incoming_messages.pop_front();
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