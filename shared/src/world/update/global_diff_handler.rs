use std::{collections::HashMap, net::SocketAddr};

use crate::{CachedComponentUpdate, ComponentKind, ComponentKinds, GlobalEntity, GlobalWorldManagerType};

use crate::world::update::global_entity_index::GlobalEntityIndex;
use crate::world::update::mut_channel::{MutChannel, MutReceiver, MutReceiverBuilder, MutSender};

/// Per-entity component metadata. One bool per registered kind_bit.
/// `user_dependent[kind_bit] == true` iff the component at that bit position
/// has `EntityProperty` fields and therefore produces per-user-distinct wire bytes.
/// Indexed by `GlobalEntityIndex`; slot 0 unused (INVALID sentinel).
pub struct ComponentFlags {
    user_dependent: Vec<bool>,
}

impl ComponentFlags {
    fn new(kind_count: usize) -> Self {
        Self { user_dependent: vec![false; kind_count] }
    }

    fn set_user_dependent(&mut self, kind_bit: u16, value: bool) {
        let idx = kind_bit as usize;
        if idx >= self.user_dependent.len() {
            self.user_dependent.resize(idx + 1, false);
        }
        self.user_dependent[idx] = value;
    }
}

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
    /// Per-entity component metadata: idx_to_components[slot].user_dependent[kind_bit]
    /// is true iff that component has EntityProperty fields. O(1) array access in Phase 2.
    idx_to_components: Vec<ComponentFlags>,
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
            idx_to_components: Vec::new(),
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

    /// Pre-populates `max_kind_count` from the static `ComponentKinds` total at
    /// server/client startup, before any connection is accepted. This ensures
    /// `UserDiffHandler::new()` reads a non-zero stride even when no entities
    /// have been registered yet.
    pub fn set_protocol_kind_count(&mut self, count: u16) {
        if count > self.max_kind_count {
            self.max_kind_count = count;
        }
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

        let kind_bit_opt = if let std::collections::hash_map::Entry::Vacant(entry) =
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
                Some(net_id)
            } else {
                None
            }
        } else {
            self.kind_bits.get(component_kind).copied()
        };

        // Record per-entity user_dependent flag for O(1) Phase-2 path selection.
        if let Some(kind_bit) = kind_bit_opt {
            if let Some(&entity_idx) = self.global_to_idx.get(global_entity) {
                let slot = entity_idx.0 as usize;
                let is_user_dep = component_kinds.is_user_dependent(component_kind);
                if let Some(flags) = self.idx_to_components.get_mut(slot) {
                    flags.set_user_dependent(kind_bit, is_user_dep);
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
        let kind_count = self.max_kind_count as usize;
        if slot >= self.idx_to_global.len() {
            self.idx_to_global.resize(slot + 1, None);
        }
        if slot >= self.idx_to_components.len() {
            while self.idx_to_components.len() <= slot {
                self.idx_to_components.push(ComponentFlags::new(kind_count));
            }
        } else {
            // Reset stale component flags from a previously-freed slot.
            self.idx_to_components[slot] = ComponentFlags::new(kind_count);
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

    /// Returns `true` if the component at `(idx, kind_bit)` has EntityProperty fields,
    /// `false` if not, or `None` if the entity or kind_bit is out of range.
    /// O(1) array access — replaces `ComponentKinds::is_user_dependent()` HashSet lookup
    /// in the Phase-2 dirty scan hot path.
    pub fn is_component_user_dependent(&self, idx: GlobalEntityIndex, kind_bit: u16) -> Option<bool> {
        self.idx_to_components
            .get(idx.0 as usize)
            .and_then(|flags| flags.user_dependent.get(kind_bit as usize))
            .copied()
    }
}
