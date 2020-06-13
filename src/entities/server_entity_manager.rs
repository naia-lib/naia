
use std::{
    collections::{VecDeque, HashMap},
    rc::Rc,
    cell::RefCell,
    net::SocketAddr,
    borrow::{Borrow},
    clone::Clone
};

use slotmap::{SparseSecondaryMap};

use gaia_shared::{EntityType, LocalEntityKey, NetEntity, StateMask, EntityNotifiable};
use super::{
    server_entity_message::ServerEntityMessage,
    entity_record::{
        LocalEntityStatus,
        EntityRecord,
    },
    entity_key::EntityKey,
    mut_handler::MutHandler,
    //server_entity::ServerEntity,
};

pub struct ServerEntityManager<T: EntityType> {
    address: SocketAddr,
    local_entity_store: SparseSecondaryMap<EntityKey, Rc<RefCell<dyn NetEntity<T>>>>,
    local_to_global_key_map: HashMap<LocalEntityKey, EntityKey>,
    recycled_local_keys: Vec<LocalEntityKey>,
    next_new_local_key: LocalEntityKey,
    entity_records: SparseSecondaryMap<EntityKey, EntityRecord>,
    queued_messages: VecDeque<ServerEntityMessage<T>>,
    sent_messages: HashMap<u16, Vec<ServerEntityMessage<T>>>,
    sent_updates: HashMap<u16, HashMap<EntityKey, Rc<RefCell<StateMask>>>>,
    last_update_packet_index: u16,
    last_last_update_packet_index: u16,
    mut_handler: Rc<RefCell<MutHandler>>,
    last_popped_state_mask: StateMask,
}

impl<T: EntityType> ServerEntityManager<T> {
    pub fn new(address: SocketAddr, mut_handler: &Rc<RefCell<MutHandler>>) -> Self {
        ServerEntityManager {
            address,
            local_entity_store:  SparseSecondaryMap::new(),
            local_to_global_key_map: HashMap::new(),
            recycled_local_keys: Vec::new(),
            next_new_local_key: 0,
            entity_records: SparseSecondaryMap::new(),
            queued_messages: VecDeque::new(),
            sent_messages: HashMap::new(),
            sent_updates: HashMap::<u16, HashMap<EntityKey, Rc<RefCell<StateMask>>>>::new(),
            last_update_packet_index: 0,
            last_last_update_packet_index: 0,
            mut_handler: mut_handler.clone(),
            last_popped_state_mask: StateMask::new(0),
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_messages.len() != 0;
    }

    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<ServerEntityMessage<T>> {

        match self.queued_messages.pop_front() {
            Some(message) => {
                if !self.sent_messages.contains_key(&packet_index) {
                    let sent_messages_list: Vec<ServerEntityMessage<T>> = Vec::new();
                    self.sent_messages.insert(packet_index, sent_messages_list);
                }

                if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
                    sent_messages_list.push(message.clone());
                }

                //clear state mask of entity if need be
                match &message {
                    ServerEntityMessage::Create(global_key, _, _) => {
                        if let Some(record) = self.entity_records.get(*global_key) {
                            self.last_popped_state_mask = record.get_state_mask().as_ref().borrow().clone();
                        }
                        self.mut_handler.as_ref().borrow_mut().clear_state(&self.address, global_key);
                    }
                    ServerEntityMessage::Update(global_key, local_key, state_mask, entity) => {
                        // previously the state mask was the CURRENT state mask for the entity,
                        // we want to lock that in so we know exactly what we're writing
                        let locked_state_mask = Rc::new(RefCell::new(state_mask.as_ref().borrow().clone()));

                        // place state mask in a special transmission record - like map
                        if !self.sent_updates.contains_key(&packet_index) {
                            let sent_updates_map: HashMap<EntityKey, Rc<RefCell<StateMask>>> = HashMap::new();
                            self.sent_updates.insert(packet_index, sent_updates_map);
                            self.last_last_update_packet_index = self.last_update_packet_index;
                            self.last_update_packet_index = packet_index;
                        }

                        if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
                            sent_updates_map.insert(*global_key, locked_state_mask.clone());
                        }

                        // having copied the state mask for this update, clear the state
                        self.last_popped_state_mask = state_mask.as_ref().borrow().clone();
                        self.mut_handler.as_ref().borrow_mut().clear_state(&self.address, global_key);

                        // return new Update message to be written
                        return Some(ServerEntityMessage::Update(*global_key, *local_key, locked_state_mask, entity.clone()));
                    }
                    _ => {}
                }

                return Some(message);
            }
            None => { return None; }

        }
    }

    pub fn unpop_outgoing_message(&mut self, packet_index: u16, message: &ServerEntityMessage<T>) {
        info!("unpopping");
        if let Some(sent_messages_list) = self.sent_messages.get_mut(&packet_index) {
            sent_messages_list.pop();
            if sent_messages_list.len() == 0 {
                self.sent_messages.remove(&packet_index);
            }
        }

        match &message {
            ServerEntityMessage::Create(global_key, _, _) => {
                self.mut_handler.as_ref().borrow_mut().set_state(&self.address, global_key, &self.last_popped_state_mask);
            },
            ServerEntityMessage::Update(global_key, local_key, _, entity) => {
                if let Some(sent_updates_map) = self.sent_updates.get_mut(&packet_index) {
                    sent_updates_map.remove(global_key);
                    if sent_updates_map.len() == 0 {
                        self.sent_updates.remove(&packet_index);
                    }
                }

                self.last_update_packet_index = self.last_last_update_packet_index;
                self.mut_handler.as_ref().borrow_mut().set_state(&self.address, global_key, &self.last_popped_state_mask);

                let record = self.entity_records.get(*global_key)
                    .expect("uh oh, we don't have enough info to unpop the message");
                let original_state_mask = record.get_state_mask().clone();
                let cloned_message = ServerEntityMessage::Update(*global_key, *local_key, original_state_mask, entity.clone());
                self.queued_messages.push_front(cloned_message);
                return;
            },
            _ => {}
        }

        self.queued_messages.push_front(message.clone());
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
            self.mut_handler.as_ref().borrow_mut().register_mask(&self.address, &key, entity_record.get_state_mask());
            self.entity_records.insert(key, entity_record);
            self.queued_messages.push_back(ServerEntityMessage::Create(key, local_key, entity.clone()));
        }
    }

    pub fn remove_entity(&mut self, key: EntityKey) {
        if let Some(entity_record) = self.entity_records.get_mut(key) {
            if entity_record.status != LocalEntityStatus::Deleting {
                entity_record.status = LocalEntityStatus::Deleting;
                self.queued_messages.push_back(ServerEntityMessage::Delete(key, entity_record.local_key));
            }
        }
    }

    fn get_new_local_key(&mut self) -> u16 {
        if let Some(local_key) = self.recycled_local_keys.pop() {
            return local_key;
        }

        let output = self.next_new_local_key;
        self.next_new_local_key += 1;
        return output;
    }

    pub fn collect_entity_updates(&mut self) {
        for (key, record) in self.entity_records.iter() {
            if record.status == LocalEntityStatus::Created && !record.get_state_mask().as_ref().borrow().is_clear() {
                if let Some(entity_ref) = self.local_entity_store.get(key) {
                    self.queued_messages.push_back(ServerEntityMessage::Update(key,
                                                                               record.local_key,
                                                                               record.get_state_mask().clone(),
                                                                               entity_ref.clone(),
                    ));
                }
            }
        }
    }

    pub fn get_local_entity(&self, key: u16) -> Option<&Rc<RefCell<dyn NetEntity<T>>>> {
        if let Some(global_key) = self.local_to_global_key_map.get(&key) {
            if let Some(entity) = self.local_entity_store.get(*global_key) {
                return Some(entity);
            }
        }
        return None;
    }
}

impl<T: EntityType> EntityNotifiable for ServerEntityManager<T> {
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        if let Some(delivered_messages_list) = self.sent_messages.get(&packet_index) {
            for delivered_message in delivered_messages_list.into_iter() {

                match delivered_message {
                    ServerEntityMessage::Create(global_key, _, _) => {
                        if let Some(entity_record) = self.entity_records.get_mut(*global_key) {
                            // update entity record status
                            entity_record.status = LocalEntityStatus::Created;
                        }
                    },
                    ServerEntityMessage::Delete(global_key_ref, local_key) => {
                        let global_key = *global_key_ref;
                        if let Some(_) = self.entity_records.get(global_key) {
                            // actually delete the entity from local records
                            self.mut_handler.as_ref().borrow_mut().deregister_mask(&self.address, global_key_ref);
                            self.local_entity_store.remove(global_key);
                            self.local_to_global_key_map.remove(local_key);
                            self.recycled_local_keys.push(*local_key);
                            self.entity_records.remove(global_key);
                        }
                    }
                    ServerEntityMessage::Update(_, _, _, _) => {
                        self.sent_updates.remove(&packet_index);
                    }
                }
            }

            self.sent_messages.remove(&packet_index);
        }
    }

    fn notify_packet_dropped(&mut self, dropped_packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_messages.get(&dropped_packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {

                match dropped_message {
                    ServerEntityMessage::Create(_, _, _) | ServerEntityMessage::Delete(_, _) => {
                        self.queued_messages.push_back(dropped_message.clone());
                    },
                    ServerEntityMessage::Update(global_key, _, _, _) => {

                        if let Some(state_mask_map) = self.sent_updates.get(&dropped_packet_index) {
                            if let Some(state_mask) = state_mask_map.get(global_key) {

                                let mut new_state_mask = state_mask.as_ref().borrow().clone();

                                // walk from dropped packet up to most recently sent packet
                                if dropped_packet_index != self.last_update_packet_index {
                                    let mut packet_index = dropped_packet_index.wrapping_add(1);
                                    while packet_index != self.last_update_packet_index {
                                        if let Some(state_mask_map) = self.sent_updates.get(&packet_index) {
                                            if let Some(state_mask) = state_mask_map.get(global_key) {
                                                new_state_mask.nand(state_mask.as_ref().borrow().borrow());
                                            }
                                        }

                                        packet_index = packet_index.wrapping_add(1);
                                    }
                                }

                                if let Some(record) = self.entity_records.get_mut(*global_key) {
                                    let mut current_state_mask = record.get_state_mask().as_ref().borrow_mut();
                                    current_state_mask.or(new_state_mask.borrow());
                                }
                            }
                        }
                    }
                }
            }

            self.sent_updates.remove(&dropped_packet_index);
            self.sent_messages.remove(&dropped_packet_index);
        }
    }
}