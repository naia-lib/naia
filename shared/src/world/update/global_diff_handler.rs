use std::{collections::HashMap, net::SocketAddr};

use crate::{CachedComponentUpdate, ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_entity_index::GlobalEntityIndex;
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
    // Dense entity registry — O(1) GlobalEntity ↔ GlobalEntityIndex lookups.
    global_to_idx: HashMap<GlobalEntity, GlobalEntityIndex>,
    /// Dense array: slot = GlobalEntityIndex.as_usize(). Slot 0 unused (INVALID sentinel).
    idx_to_global: Vec<Option<GlobalEntity>>,
    /// Free list for index recycling on entity despawn.
    free_list: Vec<GlobalEntityIndex>,
    /// Next fresh index to issue (starts at 1; 0 = INVALID).
    next_idx: u32,
    /// Inverse kind-bit lookup: bit_to_kind[kind_bit] = ComponentKind. O(1) hot path.
    /// Populated at register_component time. `None` for unregistered bit positions.
    bit_to_kind: Vec<Option<ComponentKind>>,
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
            global_to_idx: HashMap::new(),
            idx_to_global: Vec::new(),
            free_list: Vec::new(),
            next_idx: 1, // 0 is INVALID
            bit_to_kind: Vec::new(),
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
                // Populate inverse kind-bit lookup for O(1) hot-path access.
                let bit_idx = net_id as usize;
                if bit_idx >= self.bit_to_kind.len() {
                    self.bit_to_kind.resize(bit_idx + 1, None);
                }
                self.bit_to_kind[bit_idx] = Some(*component_kind);
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

    /// Assigns a `GlobalEntityIndex` to `global`, or returns the existing one if already allocated.
    /// O(1) amortized. Called at entity spawn time.
    pub fn alloc_entity(&mut self, global: GlobalEntity) -> GlobalEntityIndex {
        if let Some(&existing) = self.global_to_idx.get(&global) {
            return existing;
        }
        let idx = if let Some(recycled) = self.free_list.pop() {
            recycled
        } else {
            let i = self.next_idx;
            self.next_idx += 1;
            GlobalEntityIndex(i)
        };
        let slot = idx.0 as usize;
        if slot >= self.idx_to_global.len() {
            self.idx_to_global.resize(slot + 1, None);
        }
        self.idx_to_global[slot] = Some(global);
        self.global_to_idx.insert(global, idx);
        idx
    }

    /// Releases the `GlobalEntityIndex` for `global`, returning it to the free list.
    /// O(1). Called at entity despawn time. Idempotent — safe to call multiple times.
    pub fn free_entity(&mut self, global: &GlobalEntity) {
        if let Some(idx) = self.global_to_idx.remove(global) {
            if let Some(slot) = self.idx_to_global.get_mut(idx.0 as usize) {
                *slot = None;
            }
            self.free_list.push(idx);
        }
    }

    /// Returns the `GlobalEntityIndex` for `global`, or `None` if not allocated.
    pub fn entity_to_global_idx(&self, global: &GlobalEntity) -> Option<GlobalEntityIndex> {
        self.global_to_idx.get(global).copied()
    }

    /// Returns the `GlobalEntity` for `idx`, or `None` if the slot is unused or out of range.
    pub fn global_entity_at(&self, idx: GlobalEntityIndex) -> Option<GlobalEntity> {
        self.idx_to_global.get(idx.0 as usize).and_then(|e| *e)
    }

    /// Returns the `ComponentKind` for `kind_bit`, or `None` if not registered.
    /// O(1) array access — inverse of `kind_bits` map.
    pub fn kind_for_bit(&self, kind_bit: u16) -> Option<ComponentKind> {
        self.bit_to_kind.get(kind_bit as usize).and_then(|k| *k)
    }
}
