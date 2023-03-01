use std::{collections::HashMap, hash::Hash, net::SocketAddr};

use crate::ComponentKind;

use super::mut_channel::{MutChannel, MutReceiver, MutReceiverBuilder, MutSender};

pub struct GlobalDiffHandler<E: Copy + Eq + Hash> {
    mut_receiver_builders: HashMap<(E, ComponentKind), MutReceiverBuilder>,
}

impl<E: Copy + Eq + Hash> GlobalDiffHandler<E> {
    pub fn new() -> Self {
        Self {
            mut_receiver_builders: HashMap::new(),
        }
    }

    // For Server
    pub fn register_component(
        &mut self,
        entity: &E,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> MutSender {
        if self
            .mut_receiver_builders
            .contains_key(&(*entity, *component_kind))
        {
            panic!("Component cannot register with Server more than once!");
        }

        let (sender, builder) = MutChannel::new_channel(diff_mask_length);

        self.mut_receiver_builders
            .insert((*entity, *component_kind), builder);

        sender
    }

    pub fn deregister_component(&mut self, entity: &E, component_kind: &ComponentKind) {
        self.mut_receiver_builders
            .remove(&(*entity, *component_kind));
    }

    pub fn receiver(
        &self,
        addr: &SocketAddr,
        entity: &E,
        component_kind: &ComponentKind,
    ) -> Option<MutReceiver> {
        if let Some(builder) = self.mut_receiver_builders.get(&(*entity, *component_kind)) {
            return builder.build(addr);
        }
        None
    }
}
