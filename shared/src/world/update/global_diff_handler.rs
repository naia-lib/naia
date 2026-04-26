use std::{collections::HashMap, net::SocketAddr};

use crate::{ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::mut_channel::{MutChannel, MutReceiver, MutReceiverBuilder, MutSender};

pub struct GlobalDiffHandler {
    mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>,
    /// `ComponentKind` → NetId (== bit position in the per-user
    /// `DirtyQueue` u64 mask). Populated lazily as `register_component`
    /// fires; `UserDiffHandler` reads this to wire `DirtyNotifier`s with
    /// a precomputed `kind_bit`. Phase 9.4 / Stage E.
    kind_bits: HashMap<ComponentKind, u8>,
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
            kind_bits: HashMap::new(),
        }
    }

    /// NetId of a registered kind, used as bit position in the per-user
    /// `DirtyQueue` u64 mask. Returns `None` if the kind has never gone
    /// through `register_component` here.
    pub fn kind_bit(&self, component_kind: &ComponentKind) -> Option<u8> {
        self.kind_bits.get(component_kind).copied()
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
        }

        let (sender, builder) = MutChannel::new_channel(global_world_manager, diff_mask_length);

        self.mut_receiver_builders
            .insert((*global_entity, *component_kind), builder);

        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.kind_bits.entry(*component_kind)
        {
            if let Some(net_id) = component_kinds.net_id_of(component_kind) {
                entry.insert(net_id as u8);
            }
        }

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
