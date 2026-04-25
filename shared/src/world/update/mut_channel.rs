use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock, RwLock, Weak},
};

use crate::world::update::atomic_diff_mask::AtomicDiffMask;
use crate::{ComponentKind, DiffMask, GlobalEntity, GlobalWorldManagerType, PropertyMutate};

/// Per-user dirty queue.
///
/// Phase 8.1 Stage B (2026-04-25) replaces the previous
/// `RwLock<HashMap<GlobalEntity, HashSet<ComponentKind>>>` DirtySet with
/// a flat `Mutex<DirtyQueue>` whose drain semantics avoid the O(N)
/// HashMap clone the prior implementation paid every tick. Each entry is
/// pushed on its mask's clean→dirty transition and stays in `queue` until
/// a tick-time drain. `in_dirty` deduplicates pushes against repeat
/// transitions while the entry is still queued.
///
/// Wire-format invariant unchanged — this is purely a CPU-side bookkeeping
/// substitution; the eventual outgoing-update payload contents are
/// determined by the per-receiver `AtomicDiffMask`, which is the truth
/// source. Stale queue entries (dirty bit cleared after enqueue, e.g. by
/// `clear_diff_mask`) are tolerated; the caller filters them out at drain
/// time by re-checking `MutReceiver::diff_mask_is_clear`.
pub struct DirtyQueue {
    /// Flat membership of `(GlobalEntity, ComponentKind)` keys whose
    /// masks are currently non-clear. Stage B uses this directly as the
    /// dirty cache; Stage D will swap to an index-keyed bitset when
    /// `MutChannelData` plumbs `EntityIndex` end-to-end.
    in_dirty: HashSet<(GlobalEntity, ComponentKind)>,
}

impl DirtyQueue {
    pub fn new() -> Self {
        Self { in_dirty: HashSet::new() }
    }

    /// Mark `(entity, kind)` dirty. Idempotent; cheap on already-dirty
    /// entries (single HashSet probe). The pre-Stage-B implementation
    /// did `HashMap::entry(entity).or_default().insert(kind)` — two
    /// hashes plus a possible HashSet allocation per call. The flat
    /// HashSet here is one hash per call.
    pub fn push(&mut self, entity: GlobalEntity, kind: ComponentKind) {
        self.in_dirty.insert((entity, kind));
    }

    /// Cancel a dirty entry (mask cleared). Cheap removal.
    pub fn cancel(&mut self, entity: GlobalEntity, kind: ComponentKind) {
        self.in_dirty.remove(&(entity, kind));
    }

    /// Build a `HashMap<GlobalEntity, HashSet<ComponentKind>>` snapshot
    /// shaped to match the pre-Stage-B return type expected by callers.
    /// Allocates only the unique entities + their kind sets — no nested
    /// HashMap clone (the prior implementation cloned a `HashMap<_,
    /// HashSet<_>>` whose nested HashSets each entailed a per-entity
    /// re-allocation).
    pub fn snapshot(&self) -> std::collections::HashMap<GlobalEntity, HashSet<ComponentKind>> {
        let mut result: std::collections::HashMap<GlobalEntity, HashSet<ComponentKind>> =
            std::collections::HashMap::with_capacity(self.in_dirty.len());
        for &(entity, kind) in self.in_dirty.iter() {
            result.entry(entity).or_default().insert(kind);
        }
        result
    }

    pub fn len(&self) -> usize {
        self.in_dirty.len()
    }

    pub fn is_empty(&self) -> bool {
        self.in_dirty.is_empty()
    }
}

impl Default for DirtyQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared dirty queue owned by a `UserDiffHandler`. MutReceivers hold a
/// `Weak` into this and push their own `(entity, kind)` key whenever their
/// diff mask transitions clean→dirty.
pub type DirtySet = Mutex<DirtyQueue>;

/// Identifies a MutReceiver's position inside its owning UserDiffHandler's
/// dirty set. Installed once per receiver via `MutReceiver::attach_notifier`
/// (OnceLock — all clones share the notifier).
pub struct DirtyNotifier {
    entity: GlobalEntity,
    kind: ComponentKind,
    set: Weak<DirtySet>,
}

impl DirtyNotifier {
    pub fn new(
        entity: GlobalEntity,
        kind: ComponentKind,
        set: Weak<DirtySet>,
    ) -> Self {
        Self { entity, kind, set }
    }

    fn notify_dirty(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.lock() {
                guard.push(self.entity, self.kind);
            }
        }
    }

    fn notify_clean(&self) {
        if let Some(set) = self.set.upgrade() {
            if let Ok(mut guard) = set.lock() {
                guard.cancel(self.entity, self.kind);
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
