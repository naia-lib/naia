use std::{any::TypeId, collections::HashMap};

use crate::{
    entities::{entity_builder::EntityBuilder, entity_type::EntityType},
    events::{event_builder::EventBuilder, event_type::EventType},
    packet_reader::PacketReader,
};

/// Contains the shared protocol between Client & Server, with a data that is
/// able to map Event/Entity TypeIds to their representation within specified
/// enums. Also is able to create new Event/Entities using registered Builders,
/// given a specific TypeId.
#[derive(Debug)]
pub struct Manifest<T: EventType, U: EntityType> {
    event_naia_id_count: u16,
    event_builder_map: HashMap<u16, Box<dyn EventBuilder<T>>>,
    event_type_map: HashMap<TypeId, u16>,
    ////
    entity_naia_id_count: u16,
    entity_builder_map: HashMap<u16, Box<dyn EntityBuilder<U>>>,
    entity_type_map: HashMap<TypeId, u16>,
}

impl<T: EventType, U: EntityType> Manifest<T, U> {
    /// Create a new Manifest
    pub fn new() -> Self {
        Manifest {
            event_naia_id_count: 0,
            event_builder_map: HashMap::new(),
            event_type_map: HashMap::new(),
            ///
            entity_naia_id_count: 0,
            entity_builder_map: HashMap::new(),
            entity_type_map: HashMap::new(),
        }
    }

    /// Register an EventBuilder to handle the creation of Event instances
    pub fn register_event(&mut self, event_builder: Box<dyn EventBuilder<T>>) {
        let new_naia_id = self.event_naia_id_count;
        let type_id = event_builder.get_type_id();
        self.event_type_map.insert(type_id, new_naia_id);
        self.event_builder_map.insert(new_naia_id, event_builder);
        self.event_naia_id_count += 1;
    }

    /// Given an Event's TypeId, get a NaiaId (that can be written/read from
    /// packets)
    pub fn get_event_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .event_type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    /// Creates an Event instance, given a NaiaId and a payload, typically from
    /// an incoming packet
    pub fn create_event(&self, naia_id: u16, reader: &mut PacketReader) -> Option<T> {
        match self.event_builder_map.get(&naia_id) {
            Some(event_builder) => {
                return Some(event_builder.as_ref().build(reader));
            }
            None => {}
        }

        return None;
    }

    /// Register an EntityBuilder to handle the creation of Entity instances
    pub fn register_entity(&mut self, entity_builder: Box<dyn EntityBuilder<U>>) {
        let new_naia_id = self.entity_naia_id_count;
        let type_id = entity_builder.get_type_id();
        self.entity_type_map.insert(type_id, new_naia_id);
        self.entity_builder_map.insert(new_naia_id, entity_builder);
        self.entity_naia_id_count += 1;
    }

    /// Given an Entity's TypeId, get a NaiaId (that can be written/read from
    /// packets)
    pub fn get_entity_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .entity_type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    /// Creates an Event instance, given a NaiaId and a payload, typically from
    /// an incoming packet
    pub fn create_entity(&self, naia_id: u16, reader: &mut PacketReader) -> Option<U> {
        match self.entity_builder_map.get(&naia_id) {
            Some(entity_builder) => {
                return Some(entity_builder.as_ref().build(reader));
            }
            None => {}
        }

        return None;
    }

    /// Register both an EntityBuilder and an EventBuilder to handle the
    /// creation of both as a Pawn & Command, respectively. Pawns & Commands
    /// should be used for any player-controlled entity that requires clientside
    /// prediction
    pub fn register_pawn(
        &mut self,
        entity_builder: Box<dyn EntityBuilder<U>>,
        event_builder: Box<dyn EventBuilder<T>>,
    ) {
        self.register_entity(entity_builder);
        self.register_event(event_builder);
    }
}
