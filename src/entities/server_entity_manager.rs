
use std::{
    collections::{VecDeque, HashMap},
    rc::Rc,
    cell::RefCell};

use slotmap::{SlotMap, SecondaryMap, SparseSecondaryMap};

use crate::{EntityType, EntityKey, PacketReader, EntityManifest, NetEntity, EntityStore, LocalEntityKey, EntityRecord, EntityMessage};
use std::borrow::Borrow;

pub struct ServerEntityManager<T: EntityType> {
    local_entity_store: SparseSecondaryMap<EntityKey, Rc<RefCell<dyn NetEntity<T>>>>,
    local_to_global_key_map: SlotMap<LocalEntityKey, EntityKey>,
    entity_records: SparseSecondaryMap<EntityKey, EntityRecord>,
    queued_messages: VecDeque<EntityMessage<T>>,
    sent_messages: HashMap<u16, Vec<EntityMessage<T>>>
}

impl<T: EntityType> ServerEntityManager<T> {
    pub fn new() -> Self {
        ServerEntityManager {
            local_entity_store:  SparseSecondaryMap::new(),
            local_to_global_key_map: SlotMap::with_key(),
            entity_records: SparseSecondaryMap::new(),
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
        }
    }

    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.sent_messages.remove(&packet_index);
        //TODO: Update EntityRecord with status
    }

    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                //TODO: if an Update Packet, do not just immediately re-queue the message
                self.queued_messages.push_back(dropped_message.clone());
            }

            self.sent_messages.remove(&packet_index);
        }
    }

    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &EntityManifest<T>) {

    }

    pub fn has_entity(&self, key: EntityKey) -> bool {
        return self.local_entity_store.contains_key(key);
    }

    pub fn add_entity(&mut self, key: EntityKey, entity: &Rc<RefCell<dyn NetEntity<T>>>) {
        self.local_entity_store.insert(key, entity.clone());
        let local_key = self.local_to_global_key_map.insert(key);
        let state_mask_size = entity.as_ref().borrow().get_state_mask_size();
        let entity_record = EntityRecord::new(local_key, state_mask_size);
        self.entity_records.insert(key, entity_record);
    }

    pub fn remove_entity(&mut self, key: EntityKey) {
        self.local_entity_store.remove(key);
        let some_record = self.entity_records.get(key);
        if let Some(record) = some_record {
            self.local_to_global_key_map.remove(record.local_key);
            self.entity_records.remove(key);
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_event(&mut self, packet_index: u16) -> Option<EntityMessage<T>> {
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
}