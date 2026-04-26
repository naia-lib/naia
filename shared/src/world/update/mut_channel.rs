use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock, RwLock, Weak,
    },
};

use parking_lot::{Mutex as PlMutex, RwLock as PlRwLock};

use crate::world::entity_index::EntityIndex;
use crate::world::update::atomic_diff_mask::AtomicDiffMask;
use crate::{DiffMask, GlobalWorldManagerType, PropertyMutate};

/// Per-user dirty queue (Phase 9.4 / Stage E + B-strict).
///
/// **B-strict (2026-04-25):** the hot-path `notify_dirty` chain no longer
/// takes a `Mutex<DirtyQueue>` write lock. The bits side is a
/// `Vec<AtomicU64>` (one slot per `EntityIndex`, one bit per
/// `ComponentKind`); `push` and `cancel` are pure `fetch_or` /
/// `fetch_and` calls under a parking_lot `RwLock` *read* guard, so a
/// resize (cold path) is the only operation that excludes the hot path.
/// `dirty_indices` is locked only on the first-bit-set transition per
/// entity (the cold-path push) and at drain.
///
/// The bits Vec grows only via `ensure_capacity`, called from
/// `UserDiffHandler::allocate_entity_index` before any mutation can
/// reference the new slot. `cancel` is fire-and-forget — it clears the
/// bit but leaves any stale index in `dirty_indices`; drain dedupes via
/// the bitset.
///
/// Wire-format invariant unchanged — this is CPU-only bookkeeping.
pub struct DirtyQueue {
    /// `bits[entity_idx]` is a u64 with bit `kind_bit` set for every
    /// component kind currently dirty on that entity. Resized only by
    /// `ensure_capacity` under the `RwLock` write guard; hot-path
    /// `fetch_or`/`fetch_and`/`swap` access individual slots under the
    /// read guard.
    bits: PlRwLock<Vec<AtomicU64>>,
    /// Cold-path-only mutex: locked at first-bit-set-per-entity push and
    /// at drain. Tolerates duplicate entries — drain dedupes via `bits`.
    indices: PlMutex<Vec<EntityIndex>>,
}

impl DirtyQueue {
    pub fn new() -> Self {
        Self {
            bits: PlRwLock::new(Vec::new()),
            indices: PlMutex::new(Vec::new()),
        }
    }

    /// Pre-grow `bits` to cover at least `slot + 1` entries. Cold path —
    /// called from `UserDiffHandler::allocate_entity_index` synchronously
    /// before the issued `EntityIndex` is exposed to any mutation. Takes
    /// the write guard, which excludes hot-path readers; safe because
    /// allocation runs on the same thread that issues mutations.
    pub fn ensure_capacity(&self, slot: usize) {
        // Fast path: capacity already covers slot.
        if self.bits.read().len() > slot {
            return;
        }
        let mut w = self.bits.write();
        while w.len() <= slot {
            w.push(AtomicU64::new(0));
        }
    }

    /// Mark `(entity_idx, kind_bit)` dirty. Lock-free atomic on the bits
    /// side; cold-path mutex push only when this entity was previously
    /// clean (`prev == 0`). Holds the bits read guard for the duration
    /// of the `fetch_or`; never under the indices mutex.
    #[inline]
    pub fn push(&self, entity_idx: EntityIndex, kind_bit: u8) {
        let kind_mask = 1u64 << kind_bit;
        let i = entity_idx.0 as usize;
        let prev = {
            let bits = self.bits.read();
            if let Some(slot) = bits.get(i) {
                slot.fetch_or(kind_mask, Ordering::Relaxed)
            } else {
                drop(bits);
                // Defensive: ensure capacity then retry. Should not happen
                // in production — UserDiffHandler::allocate_entity_index
                // pre-grows. Cost: one extra read + write lock pair, only
                // on misconfigured callers.
                self.ensure_capacity(i);
                let bits = self.bits.read();
                bits[i].fetch_or(kind_mask, Ordering::Relaxed)
            }
        };
        if prev == 0 {
            self.indices.lock().push(entity_idx);
        }
    }

    /// Clear `(entity_idx, kind_bit)`. Atomic `fetch_and` on the bits
    /// side; never touches the indices mutex (drain dedupes stale
    /// entries). Tolerates out-of-range slots (returns silently).
    #[inline]
    pub fn cancel(&self, entity_idx: EntityIndex, kind_bit: u8) {
        let kind_mask = 1u64 << kind_bit;
        let i = entity_idx.0 as usize;
        let bits = self.bits.read();
        if let Some(slot) = bits.get(i) {
            slot.fetch_and(!kind_mask, Ordering::Relaxed);
        }
    }

    /// Drain: take ownership of the indices list, then atomically swap-zero
    /// each slot's bits. Returns owned `(EntityIndex, kind_bits)` pairs.
    /// Slots that ended up zero (cancelled or already drained earlier) are
    /// skipped, so drain naturally dedupes stale `dirty_indices` entries.
    pub fn drain(&self) -> Vec<(EntityIndex, u64)> {
        let indices: Vec<EntityIndex> = std::mem::take(&mut *self.indices.lock());
        let mut out: Vec<(EntityIndex, u64)> = Vec::with_capacity(indices.len());
        let bits = self.bits.read();
        for idx in indices {
            if let Some(slot) = bits.get(idx.0 as usize) {
                let v = slot.swap(0, Ordering::Relaxed);
                if v != 0 {
                    out.push((idx, v));
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.indices.lock().is_empty()
    }
}

impl Default for DirtyQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared dirty queue owned by a `UserDiffHandler`. MutReceivers hold a
/// `Weak` into this and call `push` directly — `DirtyQueue` provides
/// interior mutability via its inner `RwLock`/`Mutex` so there is no
/// outer `Mutex<DirtyQueue>` wrapper. B-strict made the bits-side
/// fetch_or lock-free under a read guard.
pub type DirtySet = DirtyQueue;

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
            set.push(self.entity_idx, self.kind_bit);
        }
    }

    fn notify_clean(&self) {
        if let Some(set) = self.set.upgrade() {
            set.cancel(self.entity_idx, self.kind_bit);
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
