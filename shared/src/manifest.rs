use std::{any::TypeId, collections::HashMap};

use crate::{
    state::{state_builder::StateBuilder, protocol_type::ProtocolType},
    PacketReader,
};

/// Contains the shared protocol between Client & Server, with a data that is
/// able to map Event/State TypeIds to their representation within specified
/// enums. Also is able to create new Event/States using registered Builders,
/// given a specific TypeId.
#[derive(Debug)]
pub struct Manifest<T: ProtocolType> {
    naia_id_count: u16,
    builder_map: HashMap<u16, Box<dyn StateBuilder<T>>>,
    type_map: HashMap<TypeId, u16>,
}

impl<T: ProtocolType> Manifest<T> {
    /// Create a new Manifest
    pub fn new() -> Self {
        Manifest {
            naia_id_count: 0,
            builder_map: HashMap::new(),
            type_map: HashMap::new(),
        }
    }

    /// Register an StateBuilder to handle the creation of State instances
    pub fn register_state(&mut self, state_builder: Box<dyn StateBuilder<T>>) {
        let new_naia_id = self.naia_id_count;
        let type_id = state_builder.get_type_id();
        self.type_map.insert(type_id, new_naia_id);
        self.builder_map.insert(new_naia_id, state_builder);
        self.naia_id_count += 1;
    }

    /// Given an State's TypeId, get a NaiaId (that can be written/read from
    /// packets)
    pub fn get_naia_id(&self, type_id: &TypeId) -> u16 {
        let naia_id = self
            .type_map
            .get(type_id)
            .expect("hey I should get a TypeId here...");
        return *naia_id;
    }

    /// Creates an State instance, given a NaiaId and a payload, typically from
    /// an incoming packet
    pub fn create_state(&self, naia_id: u16, reader: &mut PacketReader) -> T {
        if let Some(state_builder) = self.builder_map.get(&naia_id) {
            return state_builder.as_ref().build(reader);
        }

        // TODO: this shouldn't panic .. could crash the server
        panic!("No StateBuilder registered for NaiaId: {}", naia_id);
    }
}
