use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};
use log::warn;

use naia_socket_shared::Instant;

use crate::{KeyGenerator, RemoteEntity};

pub type WaitlistHandle = u16;

pub struct EntityWaitlist {
    handle_store: KeyGenerator<WaitlistHandle>,
    handle_to_required_entities: HashMap<WaitlistHandle, HashSet<RemoteEntity>>,
    waiting_entity_to_handles: HashMap<RemoteEntity, HashSet<WaitlistHandle>>,
    in_scope_entities: HashSet<RemoteEntity>,
    ready_handles: HashSet<WaitlistHandle>,
    removed_handles: HashSet<WaitlistHandle>,
    handle_ttls: VecDeque<(Instant, WaitlistHandle)>,
    handle_ttl: Duration,
}

impl EntityWaitlist {
    pub fn new() -> Self {
        Self {
            handle_to_required_entities: HashMap::new(),
            handle_store: KeyGenerator::new(Duration::from_secs(60)),
            waiting_entity_to_handles: HashMap::new(),
            in_scope_entities: HashSet::new(),
            ready_handles: HashSet::new(),
            removed_handles: HashSet::new(),
            handle_ttls: VecDeque::new(),
            handle_ttl: Duration::from_secs(60),
        }
    }

    fn must_queue(&self, entities: &HashSet<RemoteEntity>) -> bool {
        !entities.is_subset(&self.in_scope_entities)
    }

    pub fn queue<T>(
        &mut self,
        entities: &HashSet<RemoteEntity>,
        waitlist_store: &mut WaitlistStore<T>,
        item: T,
    ) -> WaitlistHandle {
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

        self.handle_ttls.push_back((Instant::now(), new_handle));
        self.handle_to_required_entities
            .insert(new_handle, entities.clone());

        waitlist_store.queue(new_handle, item);

        new_handle
    }

    pub fn collect_ready_items<T>(
        &mut self,
        waitlist_store: &mut WaitlistStore<T>,
    ) -> Option<Vec<T>> {
        self.check_handle_ttls();
        waitlist_store.remove_expired_items(&mut self.removed_handles);

        if self.ready_handles.is_empty() {
            return None;
        }

        warn!("Collecting waiting message!");
        waitlist_store.collect_ready_items(&mut self.ready_handles)
    }

    pub fn add_entity(&mut self, entity: &RemoteEntity) {
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
            // push outgoing message
            self.ready_handles.insert(outgoing_handle);
            self.remove_waiting_handle(&outgoing_handle);
        }
    }

    pub fn remove_entity(&mut self, entity: &RemoteEntity) {
        self.in_scope_entities.remove(entity);
    }

    pub fn remove_waiting_handle(&mut self, handle: &WaitlistHandle) {
        // remove handle from ttl list
        if let Some(ttl_index) = self
            .handle_ttls
            .iter()
            .position(|(_, ttl_handle)| ttl_handle == handle)
        {
            self.handle_ttls.remove(ttl_index);
        }

        // remove handle from required entities map
        let entities = self.handle_to_required_entities.remove(&handle).unwrap();

        // recycle message handle
        self.handle_store.recycle_key(&handle);

        // for all associated entities, remove from waitlist
        for entity in entities {
            let mut remove = false;
            if let Some(message_set) = self.waiting_entity_to_handles.get_mut(&entity) {
                message_set.remove(&handle);
                if message_set.is_empty() {
                    remove = true;
                }
            }
            if remove {
                self.waiting_entity_to_handles.remove(&entity);
            }
        }
    }

    fn check_handle_ttls(&mut self) {
        loop {
            let Some((ttl, _)) = self.handle_ttls.front() else {
                break;
            };
            if ttl.elapsed() < self.handle_ttl {
                break;
            }
            let (_, handle) = self.handle_ttls.pop_front().unwrap();
            self.removed_handles.insert(handle);
            self.remove_waiting_handle(&handle);
        }
    }
}

pub struct WaitlistStore<T> {
    item_handles: HashSet<WaitlistHandle>,
    items: HashMap<WaitlistHandle, T>,
}

impl<T> WaitlistStore<T> {
    pub fn new() -> Self {
        Self {
            item_handles: HashSet::new(),
            items: HashMap::new(),
        }
    }

    pub fn queue(&mut self, handle: WaitlistHandle, item: T) {
        self.item_handles.insert(handle);
        self.items.insert(handle, item);
    }

    pub fn collect_ready_items(
        &mut self,
        ready_handles: &mut HashSet<WaitlistHandle>,
    ) -> Option<Vec<T>> {
        let intersection: HashSet<WaitlistHandle> = self
            .item_handles
            .intersection(&ready_handles)
            .cloned()
            .collect();

        if intersection.len() == 0 {
            // Handles in ready_handles must refer to items in another WaitlistStore
            return None;
        }

        let mut ready_messages = Vec::new();

        for handle in intersection {
            ready_handles.remove(&handle);
            let item = self.remove(&handle).unwrap();
            ready_messages.push(item);
        }

        Some(ready_messages)
    }

    pub fn remove_expired_items(&mut self, expired_handles: &mut HashSet<WaitlistHandle>) {
        let intersection: HashSet<WaitlistHandle> = self
            .item_handles
            .intersection(&expired_handles)
            .cloned()
            .collect();

        for handle in intersection {
            expired_handles.remove(&handle);
            self.remove(&handle);
        }
    }

    pub fn remove(&mut self, handle: &WaitlistHandle) -> Option<T> {
        self.item_handles.remove(handle);
        self.items.remove(handle)
    }
}
