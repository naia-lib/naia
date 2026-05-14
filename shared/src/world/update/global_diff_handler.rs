use std::{collections::HashMap, net::SocketAddr};

use crate::{CachedComponentUpdate, ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::mut_channel::{MutChannel, MutReceiver, MutReceiverBuilder, MutSender};

/// Global registry of mutation channels for every (entity, component) pair, used to fan out property changes to per-user dirty queues.
pub struct GlobalDiffHandler {
    mut_receiver_builders: HashMap<(GlobalEntity, ComponentKind), MutReceiverBuilder>,
    /// `ComponentKind` → NetId (== bit position in the per-user
    /// `DirtyQueue`'s flat-strided bitset). Populated lazily as
    /// `register_component` fires; `UserDiffHandler` reads this to
    /// wire `DirtyNotifier`s with a precomputed `kind_bit`.
    /// Phase 9.4 / Stage E. Widened to `u16` 2026-05-05 with the
    /// unlimited-kind-count refactor (was `u8`, capped at 256 by the
    /// type even though the assertion was 64).
    kind_bits: HashMap<ComponentKind, u16>,
    /// Maximum NetId observed at registration. Used to derive the
    /// `DirtyQueue` stride at `UserDiffHandler::new` (one stride for
    /// the lifetime of the handler — protocol locks before any
    /// handler is created).
    max_kind_count: u16,
}

#[cfg(feature = "test_utils")]
impl GlobalDiffHandler {
    #[doc(hidden)]
    pub fn receiver_count(&self) -> usize {
        self.mut_receiver_builders.len()
    }

    #[doc(hidden)]
    pub fn receiver_count_by_kind(&self) -> HashMap<ComponentKind, usize> {
        let mut map = HashMap::new();
        for &(_, kind) in self.mut_receiver_builders.keys() {
            *map.entry(kind).or_insert(0) += 1;
        }
        map
    }
}

impl Default for GlobalDiffHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalDiffHandler {
    /// Creates an empty `GlobalDiffHandler`.
    pub fn new() -> Self {
        Self {
            mut_receiver_builders: HashMap::new(),
            kind_bits: HashMap::new(),
            max_kind_count: 0,
        }
    }

    /// NetId of a registered kind, used as bit position in the per-user
    /// `DirtyQueue`'s flat-strided bitset. Returns `None` if the kind
    /// has never gone through `register_component` here.
    pub fn kind_bit(&self, component_kind: &ComponentKind) -> Option<u16> {
        self.kind_bits.get(component_kind).copied()
    }

    /// Highest `kind_bit + 1` ever registered with this handler. The
    /// per-user `DirtyQueue` uses this to size its stride
    /// (`ceil(kind_count / 64)` `AtomicU64` words per entity).
    pub fn kind_count(&self) -> u16 {
        self.max_kind_count
    }

    /// Returns `true` if a mutation channel is registered for `(global_entity, component_kind)`.
    pub fn has_component(&self, global_entity: &GlobalEntity, component_kind: &ComponentKind) -> bool {
        self.mut_receiver_builders.contains_key(&(*global_entity, *component_kind))
    }

    /// Creates a `MutSender`/`MutReceiverBuilder` pair for `(global_entity, component_kind)` and returns the sender.
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
                entry.insert(net_id);
                if net_id + 1 > self.max_kind_count {
                    self.max_kind_count = net_id + 1;
                }
            }
        }

        sender
    }

    /// Removes the mutation channel for `(entity, component_kind)`, stopping further dirty notifications.
    pub fn deregister_component(&mut self, entity: &GlobalEntity, component_kind: &ComponentKind) {
        self.mut_receiver_builders
            .remove(&(*entity, *component_kind));
    }

    /// Builds a `MutReceiver` for `address` from the builder registered for `(entity, component_kind)`, if one exists.
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

    /// Returns the cached pre-serialized update for `(entity, kind, key)`, if one exists and is valid.
    pub fn get_cached_update(
        &self,
        entity: &GlobalEntity,
        kind: &ComponentKind,
        key: u64,
    ) -> Option<CachedComponentUpdate> {
        self.mut_receiver_builders
            .get(&(*entity, *kind))
            .and_then(|b| b.channel().get_cached_update(key))
    }

    /// Stores a cached pre-serialized update for `(entity, kind, key)`.
    pub fn set_cached_update(
        &self,
        entity: &GlobalEntity,
        kind: &ComponentKind,
        key: u64,
        update: CachedComponentUpdate,
    ) {
        if let Some(b) = self.mut_receiver_builders.get(&(*entity, *kind)) {
            b.channel().set_cached_update(key, update);
        }
    }
}
