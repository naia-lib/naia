use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock, RwLock, Weak,
    },
};

use parking_lot::{Mutex as PlMutex, RwLock as PlRwLock};

use crate::world::entity_index::LocalEntityIndex;
use crate::world::update::atomic_diff_mask::AtomicDiffMask;
use crate::{CachedComponentUpdate, DiffMask, GlobalWorldManagerType, PropertyMutate};

/// Per-user dirty queue (Phase 9.4 / Stage E + B-strict + 2026-05-05
/// unlimited-kind-count refactor).
///
/// Tracks, per `LocalEntityIndex`, which `ComponentKind`s are currently
/// dirty. Lock-free hot path; cold-path resize on entity allocation.
///
/// ## Variable-width kind bitset (no more 64-kind limit)
///
/// Each entity gets `stride` `AtomicU64` words of dirty bits, where
/// `stride = ceil(kind_count / 64)`. `kind_bit` of value `K` lives in
/// word `K / 64` at bit position `K % 64`. The flat layout is
/// `bits[entity_idx * stride + word_idx]` — one contiguous `Vec` for
/// all entities, each entity occupying `stride` consecutive words.
/// `stride` is set at construction from the protocol's locked
/// component-kind count and never changes.
///
/// Pre-2026-05-05 the bits were a single `Vec<AtomicU64>` (one word
/// per entity = ≤64 kinds). The cap was a `debug_assert!` in
/// `ComponentKinds::add_component`. cyberlith and other large
/// protocols were going to hit it.
///
/// ## Lock + atomic discipline
///
/// - `bits` is wrapped in `PlRwLock<Vec<AtomicU64>>`: write-guard for
///   `ensure_capacity` (cold path), read-guard for hot-path `fetch_or`
///   / `fetch_and` / `swap`. Resize is the only writer.
/// - `indices` is a cold-path `PlMutex<Vec<LocalEntityIndex>>` — locked
///   only on first-bit-set-per-entity push and at drain. Tolerates
///   duplicate entries (drain dedupes via the bitset's per-word swap).
///
/// ## "Was clear" semantics under multi-word
///
/// `push` returns `was_clear == true` (and locks `indices` to push)
/// when the kind_bit's word was zero before our `fetch_or` AND the
/// other words for this entity are also zero (relaxed loads — race-
/// tolerant). Concurrent pushes to different words of the same entity
/// might both report was_clear and double-push the index; the
/// `indices` Vec accepts duplicates and drain swap-zeroes the bits
/// once, so the duplicate entry contributes nothing on the second
/// drain pass. Net contract: at-least-once index entry per
/// clean→dirty transition, with rare benign duplicates.
///
/// Wire-format invariant unchanged — this is CPU-only bookkeeping.
pub struct DirtyQueue {
    /// Flat `Vec<AtomicU64>`, length = `entity_count * stride`. Word
    /// for `(entity_idx, word_idx)` is at index
    /// `entity_idx * stride + word_idx`. Resized only by
    /// `ensure_capacity` under the `RwLock` write guard; hot-path
    /// `fetch_or` / `fetch_and` / `swap` access individual slots
    /// under the read guard.
    bits: PlRwLock<Vec<AtomicU64>>,
    /// Words per entity = `ceil(kind_count / 64).max(1)`. Set at
    /// construction; never changes (protocol is locked before any
    /// `DirtyQueue` is created).
    stride: usize,
    /// Cold-path-only mutex: locked at first-bit-set-per-entity push
    /// and at drain. Tolerates duplicate entries — drain dedupes via
    /// `bits`.
    indices: PlMutex<Vec<LocalEntityIndex>>,
}

impl DirtyQueue {
    /// Construct with capacity for `kind_count` distinct
    /// `ComponentKind`s (= `kind_count` distinct `kind_bit` values).
    /// `stride` is derived as `ceil(kind_count / 64).max(1)`. Common
    /// case: `kind_count ≤ 64` → `stride == 1` (zero-overhead vs the
    /// old single-`AtomicU64` layout).
    pub fn new(kind_count: u16) -> Self {
        let stride = ((kind_count as usize).div_ceil(64)).max(1);
        Self {
            bits: PlRwLock::new(Vec::new()),
            stride,
            indices: PlMutex::new(Vec::new()),
        }
    }

    /// Words per entity in this queue. Public for tests + bench
    /// instrumentation; production code shouldn't need it.
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Pre-grow `bits` to cover at least `slot + 1` entities. Cold
    /// path — called from `UserDiffHandler::allocate_entity_index`
    /// synchronously before the issued `LocalEntityIndex` is exposed to
    /// any mutation. Takes the write guard, which excludes hot-path
    /// readers; safe because allocation runs on the same thread that
    /// issues mutations.
    pub fn ensure_capacity(&self, slot: usize) {
        let needed = (slot + 1) * self.stride;
        if self.bits.read().len() >= needed {
            return;
        }
        let mut w = self.bits.write();
        while w.len() < needed {
            w.push(AtomicU64::new(0));
        }
    }

    /// Mark `(entity_idx, kind_bit)` dirty. Lock-free atomic on the
    /// bits side; cold-path mutex push only on clean→dirty transition
    /// for this entity. `kind_bit` widened to `u16` (was `u8`) to
    /// support arbitrary protocol kind counts.
    #[inline]
    pub fn push(&self, entity_idx: LocalEntityIndex, kind_bit: u16) {
        let word_idx = (kind_bit as usize) / 64;
        let bit_in_word = (kind_bit as u32) % 64;
        let kind_mask = 1u64 << bit_in_word;
        let entity_base = (entity_idx.0 as usize) * self.stride;
        let slot_idx = entity_base + word_idx;

        let prev = {
            let bits = self.bits.read();
            if let Some(slot) = bits.get(slot_idx) {
                slot.fetch_or(kind_mask, Ordering::Relaxed)
            } else {
                drop(bits);
                // Defensive: ensure capacity then retry. Should not
                // happen in production — `allocate_entity_index`
                // pre-grows. Cost: one extra read + write lock pair,
                // only on misconfigured callers.
                self.ensure_capacity(entity_idx.0 as usize);
                let bits = self.bits.read();
                bits[slot_idx].fetch_or(kind_mask, Ordering::Relaxed)
            }
        };

        if prev != 0 {
            return;
        }
        // Word was zero before our fetch_or. Check whether the other
        // words for this entity are also zero. Race-tolerant: if a
        // concurrent push to another word happens between our load
        // and theirs, both might report was_clear and both push to
        // `indices` — drain dedupes via the per-word swap.
        let was_clear = if self.stride == 1 {
            true
        } else {
            let bits = self.bits.read();
            (0..self.stride).all(|w| {
                if w == word_idx {
                    return true;
                }
                bits.get(entity_base + w)
                    .map(|word| word.load(Ordering::Relaxed) == 0)
                    .unwrap_or(true)
            })
        };
        if was_clear {
            self.indices.lock().push(entity_idx);
        }
    }

    /// Clear `(entity_idx, kind_bit)`. Atomic `fetch_and` on the bits
    /// side; never touches the indices mutex (drain dedupes stale
    /// entries). Tolerates out-of-range slots (returns silently).
    #[inline]
    pub fn cancel(&self, entity_idx: LocalEntityIndex, kind_bit: u16) {
        let word_idx = (kind_bit as usize) / 64;
        let bit_in_word = (kind_bit as u32) % 64;
        let kind_mask = 1u64 << bit_in_word;
        let slot_idx = (entity_idx.0 as usize) * self.stride + word_idx;
        let bits = self.bits.read();
        if let Some(slot) = bits.get(slot_idx) {
            slot.fetch_and(!kind_mask, Ordering::Relaxed);
        }
    }

    /// Drain: take ownership of the indices list, then atomically
    /// swap-zero every word of each indexed entity. Returns owned
    /// `(LocalEntityIndex, dirty_words)` pairs where `dirty_words` is a
    /// `Vec<u64>` of length `stride` (one word per kind-word). Entries
    /// that ended up zero across all words (cancelled or already
    /// drained) are skipped.
    pub fn drain(&self) -> Vec<(LocalEntityIndex, Vec<u64>)> {
        let indices: Vec<LocalEntityIndex> = std::mem::take(&mut *self.indices.lock());
        let mut out: Vec<(LocalEntityIndex, Vec<u64>)> = Vec::with_capacity(indices.len());
        let bits = self.bits.read();
        for idx in indices {
            let entity_base = (idx.0 as usize) * self.stride;
            let mut words: Vec<u64> = Vec::with_capacity(self.stride);
            let mut any = false;
            for w in 0..self.stride {
                let v = bits
                    .get(entity_base + w)
                    .map(|slot| slot.swap(0, Ordering::Relaxed))
                    .unwrap_or(0);
                if v != 0 {
                    any = true;
                }
                words.push(v);
            }
            if any {
                out.push((idx, words));
            }
        }
        out
    }

    /// Returns `true` if no entity indices are currently queued for draining.
    pub fn is_empty(&self) -> bool {
        self.indices.lock().is_empty()
    }

    /// Build dirty candidates without consuming the dirty bits (Phase 3 / C.4).
    ///
    /// Unlike `drain()`, which atomically zeroes the bits, this method reads
    /// the bits with a Relaxed load and leaves them intact. Entities that are
    /// still dirty (bits ≠ 0) are refeeded to the index list so they appear
    /// in the next call. Entities whose bits were cleared by `cancel()` are
    /// dropped from tracking — they leave the index list naturally.
    ///
    /// This replaces the old drain-then-re-push loop in
    /// `UserDiffHandler::dirty_receiver_candidates`, turning an O(ever-dirty)
    /// per-tick scan into O(currently-dirty).
    pub fn build_candidates(&self) -> Vec<(LocalEntityIndex, Vec<u64>)> {
        let raw_indices: Vec<LocalEntityIndex> = std::mem::take(&mut *self.indices.lock());
        if raw_indices.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<(LocalEntityIndex, Vec<u64>)> = Vec::with_capacity(raw_indices.len());
        let mut refeed: Vec<LocalEntityIndex> = Vec::with_capacity(raw_indices.len());
        // Deduplicate: duplicates arise when refeed from the prior call and a
        // fresh push() both add the same entity_idx before this call runs.
        let mut dedup: std::collections::HashSet<LocalEntityIndex> =
            std::collections::HashSet::with_capacity(raw_indices.len());

        {
            let bits = self.bits.read();
            for idx in raw_indices {
                if !dedup.insert(idx) {
                    continue; // already processed this entity_idx
                }
                let entity_base = (idx.0 as usize) * self.stride;
                let mut words: Vec<u64> = Vec::with_capacity(self.stride);
                let mut any = false;
                for w in 0..self.stride {
                    let v = bits
                        .get(entity_base + w)
                        .map(|slot| slot.load(Ordering::Relaxed))
                        .unwrap_or(0);
                    if v != 0 {
                        any = true;
                    }
                    words.push(v);
                }
                if any {
                    // Still dirty — keep in tracking for the next call.
                    refeed.push(idx);
                    out.push((idx, words));
                }
                // else: bits are 0 (delivered via cancel()) — don't refeed;
                // entity naturally exits dirty tracking.
            }
        } // release bits read-guard

        // Merge refeed back. Between our std::mem::take and now, push() may
        // have added new entries for fresh mutations. Extend without creating
        // duplicates with those new entries.
        if !refeed.is_empty() {
            let mut lock = self.indices.lock();
            // New-push entries since our take — collect once to skip O(n²) scan.
            let new_pushes: std::collections::HashSet<LocalEntityIndex> =
                lock.iter().copied().collect();
            for idx in refeed {
                if !new_pushes.contains(&idx) {
                    lock.push(idx);
                }
            }
        }

        out
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
/// `LocalEntityIndex` and the protocol-wide `kind_bit` (= ComponentKind's NetId)
/// — both resolved once at registration time, so notify is a Vec OR, not a
/// hash.
/// Lightweight handle installed in a [`MutReceiver`] to push dirty notifications into a [`DirtySet`] on mutation.
pub struct DirtyNotifier {
    entity_idx: LocalEntityIndex,
    kind_bit: u16,
    set: Weak<DirtySet>,
}

impl DirtyNotifier {
    /// Creates a `DirtyNotifier` that marks `(entity_idx, kind_bit)` dirty in `set` on mutation.
    pub fn new(
        entity_idx: LocalEntityIndex,
        kind_bit: u16,
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

/// Internal trait implemented by the concrete mutation channel; produces receivers and propagates property-index notifications.
pub trait MutChannelType: Send + Sync {
    /// Creates and returns a new [`MutReceiver`] bound to `address`, or `None` if the address is excluded.
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    /// Notifies all receivers that property `diff` has changed.
    fn send(&self, diff: u8);

    /// Returns the cached pre-serialized update for the given diff mask key, if valid.
    /// Returns `None` if the cache has been invalidated (component mutated since last build).
    fn get_cached_update(&self, diff_mask_key: u64) -> Option<CachedComponentUpdate> {
        unimplemented!("MutChannelType impl must override get_cached_update; diff_mask_key={}", diff_mask_key)
    }
    /// Stores a newly-built cached update for the given diff mask key.
    fn set_cached_update(&self, diff_mask_key: u64, update: CachedComponentUpdate) {
        unimplemented!("MutChannelType impl must override set_cached_update; diff_mask_key={}, bit_count={}", diff_mask_key, update.bit_count)
    }
    /// Clears ALL cached updates. Called automatically from `send()` on every mutation.
    fn clear_cached_updates(&self) {
        unimplemented!("MutChannelType impl must override clear_cached_updates")
    }
}

/// Shared mutation channel that connects a component's property mutator to all interested receivers.
#[derive(Clone)]
pub struct MutChannel {
    data: Arc<RwLock<dyn MutChannelType>>,
}

impl MutChannel {
    /// Creates a new `(MutSender, MutReceiverBuilder)` pair backed by a channel allocated through `global_world_manager`.
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

    /// Returns a new [`MutSender`] that forwards property-index notifications into this channel.
    pub fn new_sender(&self) -> MutSender {
        MutSender::new(self)
    }

    /// Creates a new [`MutReceiver`] for `address`, or `None` if the channel excludes this address.
    pub fn new_receiver(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        if let Ok(mut data) = self.data.as_ref().write() {
            return data.new_receiver(address);
        }
        None
    }

    /// Propagates a property-index notification to all receivers; returns `false` if the channel lock is poisoned.
    pub fn send(&self, property_index: u8) -> bool {
        if let Ok(data) = self.data.as_ref().read() {
            data.send(property_index);
            return true;
        }
        false
    }

    /// Returns the cached pre-serialized update for `key`, or `None` if missing or invalidated.
    pub fn get_cached_update(&self, key: u64) -> Option<CachedComponentUpdate> {
        self.data.read().ok()?.get_cached_update(key)
    }

    /// Stores a cached pre-serialized update under `key`.
    pub fn set_cached_update(&self, key: u64, update: CachedComponentUpdate) {
        if let Ok(data) = self.data.read() {
            data.set_cached_update(key, update);
        }
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
/// Per-user receiver that accumulates dirty bits for a single component and notifies the user's [`DirtySet`] on first mutation.
#[derive(Clone)]
pub struct MutReceiver {
    mask: Arc<AtomicDiffMask>,
    notifier: Arc<OnceLock<DirtyNotifier>>,
}

impl MutReceiver {
    /// Creates a `MutReceiver` with an atomic diff mask of `diff_mask_length` bytes.
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

    /// Returns `true` if no property bits are currently set in this receiver's diff mask.
    pub fn diff_mask_is_clear(&self) -> bool {
        self.mask.is_clear()
    }

    /// Marks `property_index` dirty in the diff mask, notifying the dirty queue if the mask transitions from clean to dirty.
    pub fn mutate(&self, property_index: u8) {
        let was_clear = self.mask.set_bit(property_index);
        if was_clear {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    /// ORs `other_mask` into the diff mask, notifying the dirty queue if the mask transitions from clean to dirty.
    pub fn or_mask(&self, other_mask: &DiffMask) {
        let was_clear_now_dirty = self.mask.or_with(other_mask);
        if was_clear_now_dirty {
            if let Some(n) = self.notifier.get() {
                n.notify_dirty();
            }
        }
    }

    /// Clears all bits in the diff mask and notifies the dirty queue if the mask was dirty.
    pub fn clear_mask(&self) {
        let was_dirty = self.mask.clear();
        if was_dirty {
            if let Some(n) = self.notifier.get() {
                n.notify_clean();
            }
        }
    }
}

/// Write-only handle that forwards property-mutation notifications into a [`MutChannel`].
#[derive(Clone)]
pub struct MutSender {
    channel: MutChannel,
}

impl MutSender {
    /// Creates a `MutSender` backed by `channel`.
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }
}

impl PropertyMutate for MutSender {
    fn mutate(&mut self, property_index: u8) -> bool {
        
        self.channel.send(property_index)
    }
}

/// Factory that produces per-user [`MutReceiver`]s from a shared [`MutChannel`].
pub struct MutReceiverBuilder {
    channel: MutChannel,
}

impl MutReceiverBuilder {
    /// Creates a `MutReceiverBuilder` backed by `channel`.
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    /// Builds a new [`MutReceiver`] for `address`, or `None` if the channel excludes this address.
    pub fn build(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        self.channel.new_receiver(address)
    }

    /// Returns a reference to the underlying [`MutChannel`] for cache access.
    pub fn channel(&self) -> &MutChannel {
        &self.channel
    }
}

#[cfg(test)]
mod dirty_queue_unlimited_kinds_tests {
    //! Pins the post-T1.3 invariant: the per-user `DirtyQueue` is no
    //! longer capped at 64 component kinds. The flat-strided
    //! `Vec<AtomicU64>` storage scales with `kind_count`, and
    //! `kind_bit` values ≥ 64 round-trip through `push` → `drain`.
    use super::*;
    use crate::LocalEntityIndex;
    use std::sync::Arc;

    #[test]
    fn stride_grows_with_kind_count() {
        assert_eq!(DirtyQueue::new(1).stride(), 1);
        assert_eq!(DirtyQueue::new(64).stride(), 1);
        assert_eq!(DirtyQueue::new(65).stride(), 2);
        assert_eq!(DirtyQueue::new(128).stride(), 2);
        assert_eq!(DirtyQueue::new(129).stride(), 3);
        assert_eq!(DirtyQueue::new(1024).stride(), 16);
    }

    #[test]
    fn kind_bit_above_64_round_trips() {
        let q = Arc::new(DirtyQueue::new(200));
        q.ensure_capacity(0);
        // Pre-T1.3 these kind_bits were unrepresentable (the assertion
        // in ComponentKinds::add_component capped registration at 64).
        for &kb in &[0u16, 63, 64, 65, 127, 128, 199] {
            q.push(LocalEntityIndex(0), kb);
        }
        let drained = q.drain();
        assert_eq!(drained.len(), 1);
        let (idx, words) = &drained[0];
        assert_eq!(*idx, LocalEntityIndex(0));
        assert_eq!(words.len(), q.stride());
        // Reconstruct the absolute bit positions.
        let mut bits: Vec<u16> = Vec::new();
        for (w, &word) in words.iter().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let b = remaining.trailing_zeros() as u16;
                bits.push((w as u16) * 64 + b);
                remaining &= remaining - 1;
            }
        }
        bits.sort();
        assert_eq!(bits, vec![0, 63, 64, 65, 127, 128, 199]);
    }

    #[test]
    fn cancel_clears_high_kind_bit() {
        let q = DirtyQueue::new(200);
        q.ensure_capacity(0);
        q.push(LocalEntityIndex(0), 130);
        q.cancel(LocalEntityIndex(0), 130);
        let drained = q.drain();
        // Cancel zeroes the bit; drain skips entries with all-zero words.
        assert!(drained.is_empty());
    }

    #[test]
    fn multi_word_was_clear_fires_index_push_once() {
        let q = DirtyQueue::new(200);
        q.ensure_capacity(0);
        // Two pushes to different words for the same entity. Race-tolerant
        // was_clear may push the index twice, but drain dedupes via
        // swap-zero — only one drained entry should appear.
        q.push(LocalEntityIndex(0), 5);
        q.push(LocalEntityIndex(0), 130);
        let drained = q.drain();
        assert_eq!(drained.len(), 1, "expected dedup via drain swap-zero");
        let (_, words) = &drained[0];
        assert_eq!(words[0], 1u64 << 5);
        assert_eq!(words[2], 1u64 << (130 - 128));
    }
}
