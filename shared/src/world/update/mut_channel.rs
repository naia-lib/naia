use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock, RwLock, Weak},
};

use crate::world::entity_index::EntityIndex;
use crate::world::update::atomic_diff_mask::AtomicDiffMask;
use crate::{DiffMask, GlobalWorldManagerType, PropertyMutate};

/// Per-user dirty queue (Phase 9.4 / Stage E).
///
/// Bitset over per-user `EntityIndex`. Each entity slot is a `u64` — one
/// bit per registered `ComponentKind` (max 64; asserted at registry build
/// in `ComponentKinds::add_component`). `dirty_indices` is a sparse list
/// of EntityIndices whose slots are non-zero; it tolerates duplicates and
/// is deduped at drain time via the bitset.
///
/// Replaces Stage B's `HashSet<(GlobalEntity, ComponentKind)>` — every
/// mutation now pays a Vec index + bitwise OR + (cold-path) push instead
/// of a hash insert. The receiver's `notify_dirty` callback fires only on
/// clean→dirty transitions of the `AtomicDiffMask`, so push frequency is
/// already minimal; this stage cuts the per-push CPU cost.
///
/// Wire-format invariant unchanged — Stage E is CPU-only bookkeeping.
/// Stale entries (dirty bit cleared via `clear_diff_mask`) are tolerated;
/// `dirty_receiver_candidates` rebuilds the entity/kind keys from the
/// stored maps and the caller re-checks `diff_mask_is_clear` at drain
/// time.
pub struct DirtyQueue {
    /// `dirty_bits[entity_idx]` is a u64 with bit `kind_bit` set for every
    /// component kind currently dirty on that entity. Grows on demand.
    pub(crate) dirty_bits: Vec<u64>,
    /// Sparse list of dirty entity indices. Pushed on the first
    /// kind-bit-set-on-this-entity transition (`slot 0 → kind_mask`).
    /// May contain duplicates if an entity goes clean→dirty→clean→dirty
    /// across drains; drain handles the dedup via `dirty_bits`.
    pub(crate) dirty_indices: Vec<EntityIndex>,
}

impl DirtyQueue {
    pub fn new() -> Self {
        Self { dirty_bits: Vec::new(), dirty_indices: Vec::new() }
    }

    /// Mark `(entity_idx, kind_bit)` dirty. Vec index + bitwise OR.
    /// Pushes `entity_idx` into `dirty_indices` only on the first bit
    /// set on this entity (the slot was zero before).
    #[inline]
    pub fn push(&mut self, entity_idx: EntityIndex, kind_bit: u8) {
        let kind_mask = 1u64 << kind_bit;
        let i = entity_idx.0 as usize;
        if i >= self.dirty_bits.len() {
            self.dirty_bits.resize(i + 1, 0);
        }
        let slot = &mut self.dirty_bits[i];
        let was_zero = *slot == 0;
        *slot |= kind_mask;
        if was_zero {
            self.dirty_indices.push(entity_idx);
        }
    }

    /// Clear `(entity_idx, kind_bit)`. Tolerates entries that were never
    /// set; we only clear the bit, leaving any stale index in
    /// `dirty_indices` for drain to dedupe.
    #[inline]
    pub fn cancel(&mut self, entity_idx: EntityIndex, kind_bit: u8) {
        let kind_mask = 1u64 << kind_bit;
        if let Some(slot) = self.dirty_bits.get_mut(entity_idx.0 as usize) {
            *slot &= !kind_mask;
        }
    }

    /// Drain: returns owned `(EntityIndex, kind_bits)` pairs and zeros each
    /// slot as it's read. Caller decodes the bits and the index→entity
    /// mapping. Deduplicates via the bitset — slots already zero (canceled
    /// or visited earlier in the drain) are skipped.
    pub fn drain(&mut self) -> Vec<(EntityIndex, u64)> {
        let mut out: Vec<(EntityIndex, u64)> = Vec::with_capacity(self.dirty_indices.len());
        for idx in self.dirty_indices.drain(..) {
            let slot_idx = idx.0 as usize;
            if let Some(slot) = self.dirty_bits.get_mut(slot_idx) {
                let bits = *slot;
                if bits != 0 {
                    *slot = 0;
                    out.push((idx, bits));
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.dirty_indices.is_empty()
    }
}

impl Default for DirtyQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared dirty queue owned by a `UserDiffHandler`. MutReceivers hold a
/// `Weak` into this and push their own `(entity_idx, kind_bit)` whenever
/// their diff mask transitions clean→dirty.
pub type DirtySet = Mutex<DirtyQueue>;

/// Identifies a MutReceiver's position inside its owning UserDiffHandler's
/// dirty set. Installed once per receiver via `MutReceiver::attach_notifier`
/// (OnceLock — all clones share the notifier). Carries the per-user
/// `EntityIndex` and the protocol-wide `kind_bit` (= ComponentKind's NetId)
/// — both resolved once at registration time, so notify is a Vec OR, not a
/// hash.
pub struct DirtyNotifier {
    entity_idx: EntityIndex,
    kind_bit: u8,
    set: Weak<DirtySet>,
}

impl DirtyNotifier {
    pub fn new(
        entity_idx: EntityIndex,
        kind_bit: u8,
        set: Weak<DirtySet>,
    ) -> Self {
        Self { entity_idx, kind_bit, set }
    }

    fn notify_dirty(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.lock() {
                guard.push(self.entity_idx, self.kind_bit);
            }
        }
    }

    fn notify_clean(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.lock() {
                guard.cancel(self.entity_idx, self.kind_bit);
            }
        }
    }
}

pub trait MutChannelType: Send + Sync {
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    fn send(&self, diff: u8);
}

// MutChannel
#[derive(Clone)]
pub struct MutChannel {
    data: Arc<RwLock<dyn MutChannelType>>,
}

impl MutChannel {
    pub fn new_channel(
        global_world_manager: &dyn GlobalWorldManagerType,
        diff_mask_length: u8,
    ) -> (MutSender, MutReceiverBuilder) {
        let channel = Self {
            data: global_world_manager.new_mut_channel(diff_mask_length),
        };

        let sender = channel.new_sender();

        let builder = MutReceiverBuilder::new(&channel);

        (sender, builder)
    }

    pub fn new_sender(&self) -> MutSender {
        MutSender::new(self)
    }

    pub fn new_receiver(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        if let Ok(mut data) = self.data.as_ref().write() {
            return data.new_receiver(address);
        }
        None
    }

    pub fn send(&self, property_index: u8) -> bool {
        if let Ok(data) = self.data.as_ref().read() {
            data.send(property_index);
            return true;
        }
        false
    }
}

// MutReceiver — atomic, lock-free hot path.
//
// Phase 8.1 Stage C (2026-04-25): replaced `Arc<RwLock<DiffMask>>` with
// `Arc<AtomicDiffMask>`. `mutate(prop_idx)` is now a single atomic
// `fetch_or` instead of a `RwLock::write` + `Vec<u8>::set`-bit dance. The
// `was_clear` signal that gates `notify_dirty` is the same `prev == 0`
// check the atomic returns — semantics are byte-for-byte identical to
// the prior implementation, but the per-mutation cost drops from a
// lock-acquire round trip to one cache-line atomic.
//
// `Arc` is retained only because each user clones the same receiver via
// `MutChannelData::new_receiver`, so the inner mask must be shared. The
// notifier is `Arc<OnceLock<...>>` for the same reason.
#[derive(Clone)]
pub struct MutReceiver {
    mask: Arc<AtomicDiffMask>,
    notifier: Arc<OnceLock<DirtyNotifier>>,
}

impl MutReceiver {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            mask: Arc::new(AtomicDiffMask::new(diff_mask_length)),
            notifier: Arc::new(OnceLock::new()),
        }
    }

    /// Installed once per receiver by UserDiffHandler::register_component.
    /// Cheap no-op on re-attachment (OnceLock::set returns Err, ignored).
    pub fn attach_notifier(&self, notifier: DirtyNotifier) {
        let _ = self.notifier.set(notifier);
    }

    /// Snapshot the receiver's current mask into an owned `DiffMask`.
    /// Used by `world_writer` when copying the mask into `sent_updates`
    /// before clearing the receiver. Replaces the prior
    /// `RwLockReadGuard<'_, DiffMask>` API which forced callers to clone
    /// while holding a read lock.
    pub fn mask_snapshot(&self) -> DiffMask {
        self.mask.snapshot()
    }

    /// Read one byte of the receiver's mask. Cheaper than `mask_snapshot()`
    /// when callers only need a single byte (currently unused but kept as
    /// the obvious primitive on top of the atomic representation).
    pub fn mask_byte(&self, index: usize) -> u8 {
        self.mask.byte(index)
    }

    pub fn diff_mask_is_clear(&self) -> bool {
        self.mask.is_clear()
    }

    pub fn mutate(&self, property_index: u8) {
        let was_clear = self.mask.set_bit(property_index);
        if was_clear {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    pub fn or_mask(&self, other_mask: &DiffMask) {
        let was_clear_now_dirty = self.mask.or_with(other_mask);
        if was_clear_now_dirty {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    pub fn clear_mask(&self) {
        let was_dirty = self.mask.clear();
        if was_dirty {
            if let Some(n) = self.notifier.get() {
                n.notify_clean();
            }
        }
    }
}

// MutSender
#[derive(Clone)]
pub struct MutSender {
    channel: MutChannel,
}

impl MutSender {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }
}

impl PropertyMutate for MutSender {
    fn mutate(&mut self, property_index: u8) -> bool {
        let success = self.channel.send(property_index);
        success
    }
}

// MutReceiverBuilder
pub struct MutReceiverBuilder {
    channel: MutChannel,
}

impl MutReceiverBuilder {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    pub fn build(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        self.channel.new_receiver(address)
    }
}
