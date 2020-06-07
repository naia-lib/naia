
use crate::{EntityType, EntityKey, EntityStore, PacketReader, EntityManifest, NetEntity};
use std::{
    collections::VecDeque};

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

    }

    pub fn has_entity(&self, key: EntityKey) -> bool {
        return self.local_entity_store.has_entity(key);
    }

    pub fn add_entity(&self, key: EntityKey) {
        //return self.local_entity_store.has_entity(key);
    }

    pub fn remove_entity(&self, key: EntityKey) {
        //return self.local_entity_store.has_entity(key);
    }
}