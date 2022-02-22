use std::collections::HashMap;

use naia_socket_shared::PacketReader;

use super::{
    protocolize::{ProtocolKindType, Protocolize},
    replica_builder::ReplicaBuilder,
};

/// Contains the shared protocol between Client & Server, with a data that is
/// able to map Message/Component TypeIds to their representation within
/// specified enums. Also is able to create new Messages/Components
/// using registered Builders, given a specific TypeId.
#[derive(Clone)]
pub struct Manifest<P: Protocolize> {
    builder_map: HashMap<P::Kind, Box<dyn ReplicaBuilder<P>>>,
}

impl<P: Protocolize> Manifest<P> {
    /// Create a new Manifest
    pub fn new() -> Self {
        Manifest {
            builder_map: HashMap::new(),
        }
    }

    /// Register a ReplicaBuilder to handle the creation of
    /// Message/Component instances
    pub fn register_replica(&mut self, replica_builder: Box<dyn ReplicaBuilder<P>>) {
        self.builder_map
            .insert(replica_builder.kind(), replica_builder);
    }

    /// Creates a Message/Component instance, given a NaiaId and a
    /// payload, typically from an incoming packet
    pub fn create_replica(&self, component_kind: P::Kind, reader: &mut PacketReader) -> P {
        if let Some(replica_builder) = self.builder_map.get(&component_kind) {
            return replica_builder.as_ref().build(reader);
        }

        // TODO: this shouldn't panic .. could crash the server
        panic!(
            "No ReplicaBuilder registered for NaiaId: {}",
            component_kind.to_u16()
        );
    }
}
