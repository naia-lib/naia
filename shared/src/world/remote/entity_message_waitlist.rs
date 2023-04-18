use std::{
    collections::{HashMap, HashSet},
};

use crate::{KeyGenerator, LocalEntity};

type Handle = u16;

pub struct EntityWaitlist {
    handle_store: KeyGenerator<Handle>,
    handle_to_required_entities: HashMap<Handle, HashSet<LocalEntity>>,
    waiting_entity_to_handles: HashMap<LocalEntity, HashSet<Handle>>,
    in_scope_entities: HashSet<LocalEntity>,
    ready_handles: HashSet<Handle>,
}

impl EntityWaitlist {
    pub fn new() -> Self {
        Self {
            handle_to_required_entities: HashMap::new(),
            handle_store: KeyGenerator::new(),
            waiting_entity_to_handles: HashMap::new(),
            in_scope_entities: HashSet::new(),
            ready_handles: HashSet::new(),
        }
    }

    pub fn queue<T>(
        &mut self,
        entities: HashSet<LocalEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        item: T,
    ) {
        let new_handle = self.handle_store.generate();

        for entity in &entities {
            if !self.waiting_entity_to_handles.contains_key(entity) {
                self.waiting_entity_to_handles.insert(*entity, HashSet::new());
            }
            if let Some(message_set) = self.waiting_entity_to_handles.get_mut(entity) {
                message_set.insert(new_handle);
            }
        }

        self.handle_to_required_entities.insert(new_handle, entities);

        waitlist_store.queue(new_handle, item);
    }

    pub fn add_entity(&mut self, entity: &LocalEntity) {
        // put new entity into scope
        self.in_scope_entities.insert(*entity);

        // get a list of handles ready to send
        let mut outgoing_handles = Vec::new();

        if let Some(message_set) = self.waiting_entity_to_handles.get_mut(entity) {
            for message_handle in message_set.iter() {
                if let Some(entities) = self.handle_to_required_entities.get(message_handle) {
                    if entities.is_subset(&self.in_scope_entities) {
                        outgoing_handles.push(*message_handle);
                    }
                }
            }
        }

        // get the messages ready to send, also clean up
        for outgoing_handle in outgoing_handles {
            let entities =
                self.handle_to_required_entities.remove(&outgoing_handle).unwrap();

            // push outgoing message
            self.ready_handles.insert(outgoing_handle);

            // recycle message handle
            self.handle_store
                .recycle_key(&outgoing_handle);

            // for all associated entities, remove from waitlist
            for entity in entities {
                let mut remove = false;
                if let Some(message_set) = self.waiting_entity_to_handles.get_mut(&entity) {
                    message_set.remove(&outgoing_handle);
                    if message_set.is_empty() {
                        remove = true;
                    }
                }
                if remove {
                    self.waiting_entity_to_handles.remove(&entity);
                }
            }
        }
    }

    pub fn remove_entity(&mut self, entity: &LocalEntity) {
        // TODO: should we de-queue all our waiting messages that depend on this Entity?
        self.in_scope_entities.remove(entity);
    }

    pub fn collect_ready_items<T>(&mut self, waitlist_store: &mut WaitlistStore<T>) -> Option<Vec<T>> {
        waitlist_store.collect_ready_items(&mut self.ready_handles)
    }
}

pub struct WaitlistStore<T> {
    items: HashMap<Handle, T>,
}

impl<T> WaitlistStore<T> {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    pub fn queue(&mut self, handle: Handle, item: T) {
        self.items.insert(handle, item);
    }

    pub fn collect_ready_items(&mut self, ready_handles: &mut HashSet<Handle>) -> Option<Vec<T>> {

        let mut intersection: HashSet<Handle> = HashSet::new();

        for handle in self.items.keys() {
            if ready_handles.remove(handle) {
                intersection.insert(*handle);
            }
        }

        if intersection.len() == 0 {
            return None;
        }

        let mut ready_messages = Vec::new();

        for handle in intersection {
            ready_messages.push(self.items.remove(&handle).unwrap());
        }

        Some(ready_messages)
    }
}