use std::{collections::HashMap, net::SocketAddr};

use super::{keys::ComponentKey, mutcaster::{Mutcaster, MutSender, MutReceiver, MutReceiverBuilder}};

pub struct GlobalDiffHandler {
    mut_receiver_builders: HashMap<ComponentKey, MutReceiverBuilder>,
}

impl GlobalDiffHandler {
    pub fn new() -> GlobalDiffHandler {
        GlobalDiffHandler {
            mut_receiver_builders: HashMap::new(),
        }
    }

    // For Server
    pub fn register_component(&mut self, component_key: &ComponentKey) -> MutSender {
        if self
            .mut_receiver_builders
            .contains_key(component_key)
        {
            panic!("Component cannot register with Server more than once!");
        }

        let (sender, builder) = Mutcaster::new_channel();

        self.mut_receiver_builders.insert(*component_key, builder);

        return sender;
    }

    pub fn deregister_component(&mut self, component_key: &ComponentKey) {
        self.mut_receiver_builders.remove(component_key);
    }

    pub fn get_receiver(&self, component_key: &ComponentKey, addr: &SocketAddr) -> Option<MutReceiver> {
        if let Some(builder) = self.mut_receiver_builders.get(component_key) {
            return builder.build(addr);
        }
        return None;
    }
}
