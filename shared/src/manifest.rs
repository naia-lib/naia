use std::collections::HashMap;

use naia_socket_shared::PacketReader;

use super::{protocol_type::ProtocolType, replica_builder::ReplicaBuilder};

/// Contains the shared protocol between Client & Server, with a data that is
/// able to map Message/Component TypeIds to their representation within
/// specified enums. Also is able to create new Messages/Components
/// using registered Builders, given a specific TypeId.
#[derive(Debug)]
pub struct Manifest<P: ProtocolType> {
    naia_id_count: u16,
    builder_map: HashMap<u16, Box<dyn ReplicaBuilder<P>>>,
    type_map: HashMap<P::Kind, u16>,
}

impl<P: ProtocolType> Manifest<P> {
    /// Create a new Manifest
    pub fn new() -> Self {
        Manifest {
            naia_id_count: 0,
            builder_map: HashMap::new(),
            type_map: HashMap::new(),
        }
    }

    /// Register a ReplicaBuilder to handle the creation of
    /// Message/Component instances
    pub fn register_replica(&mut self, replica_builder: Box<dyn ReplicaBuilder<P>>) {
        let new_naia_id = self.naia_id_count;
        let builder_kind = replica_builder.get_kind();
        self.type_map.insert(builder_kind, new_naia_id);
        self.builder_map.insert(new_naia_id, replica_builder);
        self.naia_id_count += 1;
    }

    /// Given a Message/Component's TypeId, get a NaiaId (that can be
    /// written/read from packets)
    pub fn get_naia_id(&self, kind: &P::Kind) -> u16 {
        let naia_id = self
            .type_map
            .get(kind)
            .expect("hey I should get a ProtocolKind here...");
        return *naia_id;
    }

    /// Creates a Message/Component instance, given a NaiaId and a
    /// payload, typically from an incoming packet
    pub fn create_replica(&self, naia_id: u16, reader: &mut PacketReader) -> P {
        if let Some(replica_builder) = self.builder_map.get(&naia_id) {
            return replica_builder.as_ref().build(reader);
        }

        // TODO: this shouldn't panic .. could crash the server
        panic!("No ReplicaBuilder registered for NaiaId: {}", naia_id);
    }
}
