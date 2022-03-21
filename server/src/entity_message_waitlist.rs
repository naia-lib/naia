use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use naia_shared::{Protocolize, KeyGenerator};

type MessageHandle = u16;

pub struct EntityMessageWaitlist<P: Protocolize, E: Copy + Eq + Hash> {
    message_handle_store: KeyGenerator<MessageHandle>,
    messages: HashMap<MessageHandle, (Vec<E>, P)>,
    waiting_entities: HashMap<E, HashSet<MessageHandle>>,
    in_scope_entities: HashSet<E>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityMessageWaitlist<P, E> {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
            message_handle_store: KeyGenerator::new(),
            waiting_entities: HashMap::new(),
            in_scope_entities: HashSet::new(),
        }
    }

    pub fn queue_message(&mut self, entities: Vec<E>, message: P) {
        let new_handle = self.message_handle_store.generate();

        for entity in &entities {
            if !self.waiting_entities.contains_key(entity) {
                self.waiting_entities.insert(*entity, HashSet::new());
            }
            if let Some(message_set) = self.waiting_entities.get_mut(entity) {
                message_set.insert(new_handle);
            }
        }

        self.messages.insert(new_handle, (entities, message));
    }

    pub fn add_entity(&mut self, entity: &E) -> Vec<P> {
        // put new entity into scope
        self.in_scope_entities.insert(*entity);

        // get a list of handles to messages ready to send
        let mut outgoing_message_handles = Vec::new();

        if let Some(message_set) = self.waiting_entities.get_mut(entity) {
            for message_handle in message_set.iter() {
                if let Some((entities, _)) = self.messages.get(message_handle) {
                    // let mut all_entities_in_scope = true;
                    // for entity in entities {
                    //     if !self.in_scope_entities.contains(entity) {
                    //         all_entities_in_scope = false;
                    //         break;
                    //     }
                    // }
                    // if all_entities_in_scope {
                    //     outgoing_message_handles.push(*message_handle);
                    // }
                    if entities.iter().all(|entity| self.in_scope_entities.contains(entity)) {
                        outgoing_message_handles.push(*message_handle);
                    }
                }
            }
        }

        // get the messages ready to send, also clean up
        let mut output = Vec::new();

        for outgoing_message_handle in outgoing_message_handles {
            let (entities, message) = self.messages.remove(&outgoing_message_handle).unwrap();

            // push outgoing message
            output.push(message);

            // recycle message handle
            self.message_handle_store.recycle_key(&outgoing_message_handle);

            // for all associated entities, remove from waitlist
            for entity in entities {
                let mut remove = false;
                if let Some(message_set) = self.waiting_entities.get_mut(&entity) {
                    message_set.remove(&outgoing_message_handle);
                    if message_set.len() == 0 {
                        remove = true;
                    }
                }
                if remove {
                    self.waiting_entities.remove(&entity);
                }
            }
        }

        return output;
    }

    pub fn remove_entity(&mut self, entity: &E) {
        self.in_scope_entities.remove(entity);
    }
}