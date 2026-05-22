use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::world::update::global_entity_index::GlobalEntityIndex;

/// Server-global dirty tracking matrix.
///
/// Three layers:
///   ref_counts:        per (entity, kind) — count of users with non-clear DiffMask
///   dirty_components:  per (entity, kind) — summary bit: ref_count > 0 ↔ bit set
///   dirty_entities:    per entity         — summary bit: any dirty_component bit set
///
/// Layout:
///   ref_counts[entity_idx * component_count + kind_bit]
///   dirty_components[entity_idx * component_stride + kind_bit / 64], bit = kind_bit % 64
///   dirty_entities[entity_idx / 64], bit = entity_idx % 64
///
/// All indices use GlobalEntityIndex values (slot 0 = INVALID sentinel, never set).
pub struct GlobalDirtyBitset {
    ref_counts: Vec<AtomicU32>,
    component_count: usize,
    dirty_components: Vec<AtomicU64>,
    component_stride: usize,
    dirty_entities: Vec<AtomicU64>,
    capacity: usize,
}

impl GlobalDirtyBitset {
    /// Pre-allocates all storage. `capacity` is the maximum `GlobalEntityIndex.as_usize()` value
    /// (inclusive), i.e. `max_replicated_entities + 1` (slot 0 unused as INVALID sentinel).
    /// `component_count` is `ComponentKinds::kind_count()` at startup.
    pub fn new(capacity: usize, component_count: usize) -> Self {
        // Guard against zero component_count (client side / tests with no registered kinds)
        let component_count = component_count.max(1);
        let component_stride = (component_count.div_ceil(64)).max(1);

        let rc_count = capacity * component_count;
        let comp_count = capacity * component_stride;
        let ent_count = capacity.div_ceil(64).max(1);

        let mut ref_counts = Vec::with_capacity(rc_count);
        for _ in 0..rc_count { ref_counts.push(AtomicU32::new(0)); }

        let mut dirty_components = Vec::with_capacity(comp_count);
        for _ in 0..comp_count { dirty_components.push(AtomicU64::new(0)); }

        let mut dirty_entities = Vec::with_capacity(ent_count);
        for _ in 0..ent_count { dirty_entities.push(AtomicU64::new(0)); }

        Self {
            ref_counts,
            component_count,
            dirty_components,
            component_stride,
            dirty_entities,
            capacity,
        }
    }

    /// Called from `DirtyNotifier::notify_dirty` — user's (entity, kind) goes clean→dirty.
    /// Increments ref-count; on 0→1 transition sets dirty_components bit and,
    /// if the entity's component word was zero, sets dirty_entities bit.
    pub fn increment(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
        let ei = entity_idx.as_usize();
        if ei == 0 || ei >= self.capacity { return; }

        let rc_idx = ei * self.component_count + kind_bit as usize;
        if rc_idx >= self.ref_counts.len() { return; }

        let prev_rc = self.ref_counts[rc_idx].fetch_add(1, Ordering::Relaxed);
        if prev_rc == 0 {
            // 0→1: mark this component dirty.
            let word_idx = ei * self.component_stride + (kind_bit as usize) / 64;
            let bit = (kind_bit as u64) % 64;
            let prev_word = self.dirty_components[word_idx].fetch_or(1u64 << bit, Ordering::Relaxed);
            if prev_word == 0 {
                // This entity had no dirty components before — set entity summary bit.
                let ent_word = ei / 64;
                let ent_bit = (ei % 64) as u64;
                self.dirty_entities[ent_word].fetch_or(1u64 << ent_bit, Ordering::Relaxed);
            }
        }
    }

    /// Called from `DirtyNotifier::notify_clean` — user's (entity, kind) goes dirty→clean.
    /// Decrements ref-count; on 1→0 transition clears dirty_components bit and,
    /// if all component words for the entity become zero, clears dirty_entities bit.
    pub fn decrement(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) {
        let ei = entity_idx.as_usize();
        if ei == 0 || ei >= self.capacity { return; }

        let rc_idx = ei * self.component_count + kind_bit as usize;
        if rc_idx >= self.ref_counts.len() { return; }

        let prev_rc = self.ref_counts[rc_idx].fetch_sub(1, Ordering::Relaxed);
        if prev_rc == 1 {
            // 1→0: clear this component's dirty bit.
            let word_idx = ei * self.component_stride + (kind_bit as usize) / 64;
            let bit = (kind_bit as u64) % 64;
            let prev_word = self.dirty_components[word_idx].fetch_and(!(1u64 << bit), Ordering::Relaxed);
            let after_clear = prev_word & !(1u64 << bit);
            if after_clear == 0 {
                // This component word is now zero; check all words for this entity.
                let entity_fully_clean = (0..self.component_stride).all(|w| {
                    self.dirty_components[ei * self.component_stride + w]
                        .load(Ordering::Relaxed) == 0
                });
                if entity_fully_clean {
                    let ent_word = ei / 64;
                    let ent_bit = (ei % 64) as u64;
                    self.dirty_entities[ent_word].fetch_and(!(1u64 << ent_bit), Ordering::Relaxed);
                }
            }
        }
    }

    /// Returns `true` if this (entity, kind) is dirty for any user. O(1).
    pub fn is_component_dirty(&self, entity_idx: GlobalEntityIndex, kind_bit: u16) -> bool {
        let ei = entity_idx.as_usize();
        if ei == 0 || ei >= self.capacity { return false; }
        let rc_idx = ei * self.component_count + kind_bit as usize;
        if rc_idx >= self.ref_counts.len() { return false; }
        self.ref_counts[rc_idx].load(Ordering::Relaxed) > 0
    }

    /// Iterates entities with any dirty component. O(capacity / 64).
    pub fn dirty_entity_iter(&self) -> impl Iterator<Item = GlobalEntityIndex> + '_ {
        self.dirty_entities.iter().enumerate().flat_map(|(word_idx, word_cell)| {
            let word = word_cell.load(Ordering::Relaxed);
            DirtyBitIter { word, base: word_idx * 64 }
        })
    }

    /// Returns the component-level dirty words for one entity.
    /// Slice length = component_stride. Bit kind_bit%64 in word kind_bit/64 is set
    /// iff this component is dirty for at least one user.
    pub fn dirty_words(&self, entity_idx: GlobalEntityIndex) -> &[AtomicU64] {
        let start = entity_idx.as_usize() * self.component_stride;
        &self.dirty_components[start..start + self.component_stride]
    }

    /// Returns the entity-summary dirty word slice (one bit per entity index).
    /// Word `i` covers entities `i*64 .. i*64+63`.
    /// Used by `ConnectionVisibilityBitset::intersect_dirty` for word-by-word AND.
    pub fn dirty_entity_words(&self) -> &[AtomicU64] {
        &self.dirty_entities
    }

    /// Capture a plain-`u64` frozen snapshot of the current dirty state, for
    /// the active-path send worker to iterate concurrently with the gameplay
    /// thread mutating the live bitset for the next tick (MISSION_TICK_FLOOR
    /// Lever 3: the "what-to-send" set must be frozen into the send job so the
    /// lagged transmit doesn't read a torn live `global_dirty`).
    ///
    /// Sparse + cheap by design — built on the gameplay thread at snapshot-build
    /// time, so it copies only the *dirty* entities' component words (O(dirty
    /// entities), not O(capacity)). The deterministic oracle never freezes (it
    /// sends synchronously against the live bitset within the same tick).
    pub fn freeze(&self) -> FrozenGlobalDirty {
        let dirty_entity_words: Vec<u64> =
            self.dirty_entities.iter().map(|w| w.load(Ordering::Relaxed)).collect();
        let mut component_words: HashMap<u32, Vec<u64>> = HashMap::new();
        for idx in self.dirty_entity_iter() {
            let words: Vec<u64> =
                self.dirty_words(idx).iter().map(|w| w.load(Ordering::Relaxed)).collect();
            component_words.insert(idx.as_usize() as u32, words);
        }
        FrozenGlobalDirty {
            dirty_entity_words,
            component_words,
            zero_words: vec![0u64; self.component_stride],
        }
    }
}

/// Immutable, `Send + Sync` plain-`u64` snapshot of a [`GlobalDirtyBitset`]'s
/// dirty state at one instant, produced by [`GlobalDirtyBitset::freeze`].
///
/// Mirrors the three reads `send_all_packets` performs against the live
/// bitset — `dirty_entity_iter` (Phase 1/2 driver), `dirty_words` (per-entity
/// component words), and `dirty_entity_words` (the `intersect_dirty` AND) — so
/// the active send path can iterate a frozen "what-to-send" set instead of the
/// live, concurrently-mutated `global_dirty`. Sparse: only dirty entities'
/// words are stored; `dirty_words` returns a shared zero slice for the rest.
pub struct FrozenGlobalDirty {
    dirty_entity_words: Vec<u64>,
    component_words: HashMap<u32, Vec<u64>>,
    zero_words: Vec<u64>,
}

impl FrozenGlobalDirty {
    /// Mirror of [`GlobalDirtyBitset::dirty_entity_iter`] over the frozen words.
    pub fn dirty_entity_iter(&self) -> impl Iterator<Item = GlobalEntityIndex> + '_ {
        self.dirty_entity_words.iter().enumerate().flat_map(|(word_idx, word)| {
            DirtyBitIter { word: *word, base: word_idx * 64 }
        })
    }

    /// Mirror of [`GlobalDirtyBitset::dirty_words`]: the entity's frozen
    /// component-dirty words, or a shared zero slice if the entity was clean.
    /// Slice length is always `component_stride` (matching the live API).
    pub fn dirty_words(&self, entity_idx: GlobalEntityIndex) -> &[u64] {
        self.component_words
            .get(&(entity_idx.as_usize() as u32))
            .map(|v| v.as_slice())
            .unwrap_or(&self.zero_words)
    }

    /// Mirror of [`GlobalDirtyBitset::dirty_entity_words`] (plain `u64`).
    pub fn dirty_entity_words(&self) -> &[u64] {
        &self.dirty_entity_words
    }
}

struct DirtyBitIter {
    word: u64,
    base: usize,
}

impl Iterator for DirtyBitIter {
    type Item = GlobalEntityIndex;

    fn next(&mut self) -> Option<Self::Item> {
        if self.word == 0 {
            return None;
        }
        let bit = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1; // clear lowest set bit
        Some(GlobalEntityIndex((self.base + bit) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_marks_entity_dirty() {
        let gdb = GlobalDirtyBitset::new(128, 16);
        let entity = GlobalEntityIndex(1);
        let kind_bit = 0u16;

        gdb.increment(entity, kind_bit);

        assert!(gdb.is_component_dirty(entity, kind_bit));
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(dirty.contains(&entity), "entity should appear in dirty_entity_iter");
    }

    #[test]
    fn freeze_mirrors_live_dirty_state() {
        // Cross 64-bit word boundaries on BOTH the entity summary (idx 70) and
        // the component words (kind_bit 65) to exercise the multi-word layout.
        let gdb = GlobalDirtyBitset::new(200, 130);
        let idx1 = GlobalEntityIndex(1);
        let idx70 = GlobalEntityIndex(70);
        gdb.increment(idx1, 0);
        gdb.increment(idx1, 65);
        gdb.increment(idx70, 3);

        let frozen = gdb.freeze();

        // dirty_entity_iter: same set.
        let mut live: Vec<u32> = gdb.dirty_entity_iter().map(|i| i.0).collect();
        let mut froz: Vec<u32> = frozen.dirty_entity_iter().map(|i| i.0).collect();
        live.sort_unstable();
        froz.sort_unstable();
        assert_eq!(live, froz, "frozen dirty_entity_iter must match live");

        // dirty_entity_words: word-for-word equal.
        let live_words: Vec<u64> =
            gdb.dirty_entity_words().iter().map(|w| w.load(Ordering::Relaxed)).collect();
        assert_eq!(live_words.as_slice(), frozen.dirty_entity_words());

        // dirty_words: per-entity component words equal (incl. the word-65 bit).
        for idx in [idx1, idx70] {
            let live_cw: Vec<u64> =
                gdb.dirty_words(idx).iter().map(|w| w.load(Ordering::Relaxed)).collect();
            assert_eq!(live_cw.as_slice(), frozen.dirty_words(idx), "component words for {:?}", idx);
        }

        // A clean entity returns an all-zero stride-length slice (not a panic).
        let clean = frozen.dirty_words(GlobalEntityIndex(5));
        assert_eq!(clean.len(), gdb.dirty_words(GlobalEntityIndex(5)).len());
        assert!(clean.iter().all(|w| *w == 0));
    }

    #[test]
    fn decrement_to_zero_removes_entity_from_dirty() {
        let gdb = GlobalDirtyBitset::new(128, 16);
        let entity = GlobalEntityIndex(1);
        let kind_bit = 0u16;

        gdb.increment(entity, kind_bit);
        gdb.decrement(entity, kind_bit);

        assert!(!gdb.is_component_dirty(entity, kind_bit));
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(!dirty.contains(&entity), "entity should be absent after decrement to zero");
    }

    #[test]
    fn multi_user_increment_decrement() {
        let gdb = GlobalDirtyBitset::new(128, 16);
        let entity = GlobalEntityIndex(2);
        let kind_bit = 3u16;

        // Simulate 32 users marking dirty
        for _ in 0..32 {
            gdb.increment(entity, kind_bit);
        }
        assert!(gdb.is_component_dirty(entity, kind_bit));
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(dirty.contains(&entity));

        // Decrement 31 times — entity still dirty
        for _ in 0..31 {
            gdb.decrement(entity, kind_bit);
        }
        assert!(gdb.is_component_dirty(entity, kind_bit));

        // Final decrement — entity clean
        gdb.decrement(entity, kind_bit);
        assert!(!gdb.is_component_dirty(entity, kind_bit));
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(!dirty.contains(&entity));
    }

    #[test]
    fn disconnect_cleanup_invariant() {
        // Simulate: 2 users mark entity dirty, one disconnects (decrement), verify still dirty.
        // Second disconnects, verify clean.
        let gdb = GlobalDirtyBitset::new(128, 16);
        let entity = GlobalEntityIndex(5);
        let kind_bit = 1u16;

        gdb.increment(entity, kind_bit); // user A
        gdb.increment(entity, kind_bit); // user B

        // User A disconnects
        gdb.decrement(entity, kind_bit);
        assert!(gdb.is_component_dirty(entity, kind_bit), "still dirty with user B");
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(dirty.contains(&entity));

        // User B disconnects
        gdb.decrement(entity, kind_bit);
        assert!(!gdb.is_component_dirty(entity, kind_bit), "clean after both disconnect");
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(!dirty.contains(&entity));
    }

    #[test]
    fn multiple_components_on_same_entity() {
        let gdb = GlobalDirtyBitset::new(128, 16);
        let entity = GlobalEntityIndex(3);

        gdb.increment(entity, 0);
        gdb.increment(entity, 5);

        // Clean component 0, entity still dirty due to component 5
        gdb.decrement(entity, 0);
        assert!(gdb.is_component_dirty(entity, 5));
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(dirty.contains(&entity));

        // Clean component 5, entity now fully clean
        gdb.decrement(entity, 5);
        let dirty: Vec<GlobalEntityIndex> = gdb.dirty_entity_iter().collect();
        assert!(!dirty.contains(&entity));
    }
}
