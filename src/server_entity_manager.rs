
use crate::{EntityType, EntityKey, PacketReader, EntityManifest, NetEntity, EntityStore};
use std::{
    collections::VecDeque};

pub struct ServerEntityManager<T: EntityType> {
    local_entity_store: EntityStore<T>, // server should not have an entity store, as it merely references the global entity store
}

impl<T: EntityType> ServerEntityManager<T> {
    pub fn new() -> Self {
        ServerEntityManager {
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
}