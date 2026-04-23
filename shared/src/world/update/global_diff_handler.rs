use std::{collections::HashMap, net::SocketAddr};

use crate::{ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::mut_channel::{MutChannel, MutReceiver, MutReceiverBuilder, MutSender};

pub struct GlobalDiffHandler {
    mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>,
}

#[cfg(feature = "test_utils")]
impl GlobalDiffHandler {
    pub fn receiver_count(&self) -> usize {
        self.mut_receiver_builders.len()
    }

    pub fn receiver_count_by_kind(&self) -> HashMap<ComponentKind, usize> {
        let mut map = HashMap::new();
        for &(_, kind) in self.mut_receiver_builders.keys() {
            *map.entry(kind).or_insert(0) += 1;
        }
        map
    }
}

impl GlobalDiffHandler {
    pub fn new() -> Self {
        Self {
            mut_receiver_builders: HashMap::new(),
        }
    }

    pub fn register_component(
        &mut self,
        component_kinds: &ComponentKinds,
        global_world_manager: &dyn GlobalWorldManagerType,
        global_entity: &GlobalEntity,
        component_kind: &ComponentKind,
        diff_mask_length: u8,
    ) -> MutSender {
        let name = component_kinds.kind_to_name(component_kind);

        if self
            .mut_receiver_builders
            .contains_key(&(*global_entity, *component_kind))
        {
            panic!(
                "GlobalDiffHandler: For Entity {:?}, Component {} cannot Register more than once!",
                global_entity, name
            );
        } else {
            // info!(
            //     "GlobalDiffHandler: Registering Component {:?} for Entity {:?}",
            //     name, global_entity,
            // );
        }

        let (sender, builder) = MutChannel::new_channel(global_world_manager, diff_mask_length);

        self.mut_receiver_builders
            .insert((*global_entity, *component_kind), builder);

        sender
    }

    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.mut_receiver_builders
            .remove(&(*entity, *component_kind));
    }

    pub fn receiver(
        &self,
        address: &Option<SocketAddr>,
        entity: &GlobalEntity,
        component_kind: &ComponentKind,
    ) -> Option<MutReceiver> {
        if let Some(builder) = self.mut_receiver_builders.get(&(*entity, *component_kind)) {
            return builder.build(address);
        }
        None
    }
}
