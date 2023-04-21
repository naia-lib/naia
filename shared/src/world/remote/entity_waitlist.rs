use std::{
    collections::{HashMap, HashSet},
    time::Duration,
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
            handle_store: KeyGenerator::new(Duration::from_secs(60)),
            waiting_entity_to_handles: HashMap::new(),
            in_scope_entities: HashSet::new(),
            ready_handles: HashSet::new(),
        }
    }

    fn must_queue(&self, entities: &HashSet<LocalEntity>) -> bool {
        !entities.is_subset(&self.in_scope_entities)
    }

    pub fn queue<T>(
        &mut self,
        entities: &HashSet<LocalEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        item: T,
    ) -> Handle {
        let new_handle = self.handle_store.generate();

        // if all entities are in scope, we can send the message immediately
        if !self.must_queue(entities) {
            waitlist_store.queue(new_handle, item);
            self.ready_handles.insert(new_handle);
            return new_handle;
        }

        for entity in entities {
            if !self.waiting_entity_to_handles.contains_key(entity) {
                self.waiting_entity_to_handles
                    .insert(*entity, HashSet::new());
            }
            if let Some(message_set) = self.waiting_entity_to_handles.get_mut(entity) {
                message_set.insert(new_handle);
            }
        }

        self.handle_to_required_entities
            .insert(new_handle, entities.clone());

        waitlist_store.queue(new_handle, item);

        new_handle
    }

    pub fn collect_ready_items<T>(
        &mut self,
        waitlist_store: &mut WaitlistStore<T>,
    ) -> Option<Vec<T>> {
        if self.ready_handles.is_empty() {
            return None;
        }
        waitlist_store.collect_ready_items(&mut self.ready_handles)
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
            let entities = self
                .handle_to_required_entities
                .remove(&outgoing_handle)
                .unwrap();

            // push outgoing message
            self.ready_handles.insert(outgoing_handle);

            // recycle message handle
            self.handle_store.recycle_key(&outgoing_handle);

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
}

pub struct WaitlistStore<T> {
    item_handles: HashSet<Handle>,
    items: HashMap<Handle, T>,
}

impl<T> WaitlistStore<T> {
    pub fn new() -> Self {
        Self {
            item_handles: HashSet::new(),
            items: HashMap::new(),
        }
    }

    pub fn queue(&mut self, handle: Handle, item: T) {
        self.item_handles.insert(handle);
        self.items.insert(handle, item);
    }

    pub fn collect_ready_items(&mut self, ready_handles: &mut HashSet<Handle>) -> Option<Vec<T>> {

        let intersection: HashSet<Handle> = self.item_handles.intersection(&ready_handles).cloned().collect();

        if intersection.len() == 0 {
            // Handles in ready_handles must refer to items in another WaitlistStore
            return None;
        }

        let mut ready_messages = Vec::new();

        for handle in intersection {
            ready_handles.remove(&handle);
            self.item_handles.remove(&handle);
            let item = self.items.remove(&handle).unwrap();
            ready_messages.push(item);
        }

        Some(ready_messages)
    }
}
