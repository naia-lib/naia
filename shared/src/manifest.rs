use std::{any::TypeId, collections::HashMap};

use crate::{
    state::{state_builder::StateBuilder, state_type::StateType},
    events::{event_builder::EventBuilder, event_type::EventType},
    PacketReader,
};

/// Contains the shared protocol between Client & Server, with a data that is
/// able to map Event/State TypeIds to their representation within specified
/// enums. Also is able to create new Event/States using registered Builders,
/// given a specific TypeId.
#[derive(Debug)]
pub struct Manifest<T: EventType, U: StateType> {
    event_naia_id_count: u16,
    event_builder_map: HashMap<u16, Box<dyn EventBuilder<T>>>,
    event_type_map: HashMap<TypeId, u16>,
    ////
    state_naia_id_count: u16,
    state_builder_map: HashMap<u16, Box<dyn StateBuilder<U>>>,
    state_type_map: HashMap<TypeId, u16>,
}

impl<T: EventType, U: StateType> Manifest<T, U> {
    /// Create a new Manifest
    pub fn new() -> Self {
        Manifest {
            event_naia_id_count: 0,
            event_builder_map: HashMap::new(),
            event_type_map: HashMap::new(),
            ///
            state_naia_id_count: 0,
            state_builder_map: HashMap::new(),
            state_type_map: HashMap::new(),
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
        if let Some(event_builder) = self.event_builder_map.get(&naia_id) {
            return Some(event_builder.as_ref().build(reader));
        }

        return None;
    }

    /// Register an StateBuilder to handle the creation of State instances
    pub fn register_state(&mut self, state_builder: Box<dyn StateBuilder<U>>) {
        let new_naia_id = self.state_naia_id_count;
        let type_id = state_builder.get_type_id();
        self.state_type_map.insert(type_id, new_naia_id);
        self.state_builder_map.insert(new_naia_id, state_builder);
        self.state_naia_id_count += 1;
    }

    /// Given an State's TypeId, get a NaiaId (that can be written/read from
    /// packets)
    pub fn get_state_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .state_type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    /// Creates an Event instance, given a NaiaId and a payload, typically from
    /// an incoming packet
    pub fn create_state(&self, naia_id: u16, reader: &mut PacketReader) -> U {
        if let Some(state_builder) = self.state_builder_map.get(&naia_id) {
            return state_builder.as_ref().build(reader);
        }

        panic!("No StateBuilder registered for NaiaId: {}", naia_id);
    }

    /// Register both an StateBuilder and an EventBuilder to handle the
    /// creation of both as a Pawn & Command, respectively. Pawns & Commands
    /// should be used for any player-controlled state that requires clientside
    /// prediction
    pub fn register_pawn(
        &mut self,
        state_builder: Box<dyn StateBuilder<U>>,
        event_builder: Box<dyn EventBuilder<T>>,
    ) {
        self.register_state(state_builder);
        self.register_event(event_builder);
    }
}
