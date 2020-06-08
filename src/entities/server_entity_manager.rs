
use std::{
    collections::{VecDeque, HashMap},
    rc::Rc,
    cell::RefCell};

use slotmap::{SlotMap, SecondaryMap, SparseSecondaryMap};

use crate::{EntityType, EntityKey, PacketReader, EntityManifest, LocalEntityStatus, NetEntity, EntityStore, LocalEntityKey, EntityRecord, EntityMessage};
use std::borrow::Borrow;

pub struct ServerEntityManager<T: EntityType> {
    local_entity_store: SparseSecondaryMap<EntityKey, Rc<RefCell<dyn NetEntity<T>>>>,
    local_to_global_key_map: HashMap<LocalEntityKey, EntityKey>,
    recycled_local_keys: Vec<LocalEntityKey>,
    next_new_local_key: LocalEntityKey,
    entity_records: SparseSecondaryMap<EntityKey, EntityRecord>,
    queued_messages: VecDeque<EntityMessage<T>>,
    sent_messages: HashMap<u16, Vec<EntityMessage<T>>>
}

impl<T: EntityType> ServerEntityManager<T> {
    pub fn new() -> Self {
        ServerEntityManager {
            local_entity_store:  SparseSecondaryMap::new(),
            local_to_global_key_map: HashMap::new(),
            recycled_local_keys: Vec::new(),
            next_new_local_key: 0 as LocalEntityKey,
            entity_records: SparseSecondaryMap::new(),
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
        }
    }

    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
        if let Some(delivered_messages_list) = self.sent_messages.get(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {

                match delivered_message {
                    EntityMessage::Create(global_key, local_key, entity) => {
                        if let Some(entity_record) = self.entity_records.get_mut(*global_key) {
                            // update entity record status
                            entity_record.status = LocalEntityStatus::Created;
                        }
                    },
                    EntityMessage::Delete(global_key_ref, local_key) => {
                        let global_key = *global_key_ref;
                        if let Some(entity_record) = self.entity_records.get(global_key) {
                            // actually delete the entity from local records
                            self.local_entity_store.remove(global_key);
                            self.local_to_global_key_map.remove(local_key);
                            self.recycled_local_keys.push(*local_key);
                            self.entity_records.remove(global_key);
                        }
                    }
                    EntityMessage::Update(_, _) => {} //this is the right thing to do, yeah?
                }
            }

            self.sent_messages.remove(&packet_index);
        }
        //TODO: Update EntityRecord with status
    }

    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {

                match dropped_message {
                    EntityMessage::Create(_, _, _) | EntityMessage::Delete(_, _) => {
                        self.queued_messages.push_back(dropped_message.clone());
                    },
                    EntityMessage::Update(_, _) => {
                        //TODO: implement this logic.. go through state masks, ect.
                    }
                }
            }

            self.sent_messages.remove(&packet_index);
        }
    }

    pub fn has_entity(&self, key: EntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn add_entity(&mut self, key: EntityKey, entity: &Rc<RefCell<dyn NetEntity<T>>>) {
        if !self.local_entity_store.contains_key(key) {
            self.local_entity_store.insert(key, entity.clone());
            let local_key = self.get_new_local_key();
            self.local_to_global_key_map.insert(local_key, key);
            let state_mask_size = entity.as_ref().borrow().get_state_mask_size();
            let entity_record = EntityRecord::new(local_key, state_mask_size);
            self.entity_records.insert(key, entity_record);

            //TODO: queue up create entity message
            self.queued_messages.push_back(EntityMessage::Create(key, local_key, entity.clone()));
        }
    }

    pub fn remove_entity(&mut self, key: EntityKey) {
        if let Some(entity_record) = self.entity_records.get_mut(key) {
            if entity_record.status != LocalEntityStatus::Deleting {
                entity_record.status = LocalEntityStatus::Deleting;
                self.queued_messages.push_back(EntityMessage::Delete(key, entity_record.local_key));
            }
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<EntityMessage<T>> {
        match self.queued_messages.pop_front() {
            Some(message) => {
                if !self.sent_messages.contains_key(&packet_index) {
                    let sent_messages_list: Vec<EntityMessage<T>> = Vec::new();
                    self.sent_messages.insert(packet_index, sent_messages_list);
                }

                if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
                    sent_messages_list.push(message.clone());
                }

                Some(message)
            }
            None => None
        }
    }

    fn get_new_local_key(&mut self) -> LocalEntityKey {
        if let Some(local_key) = self.recycled_local_keys.pop() {
            return local_key;
        }

        let output = self.next_new_local_key;
        self.next_new_local_key += 1;
        return output;
    }
}