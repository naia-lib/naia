use std::sync::atomic::Ordering;

use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
use crate::world::update::global_entity_index::GlobalEntityIndex;

/// Per-connection bitset tracking which entities are actively in scope.
///
/// One bit per `GlobalEntityIndex`. Set when an entity enters scope (spawned into
/// the connection) and cleared when it leaves (despawned or paused via
/// `ScopeExit::Persist`). Pausing clears the bit; resuming sets it.
///
/// Sized to match `GlobalDirtyBitset` capacity (same `max_replicated_entities + 1`).
pub struct ConnectionVisibilityBitset {
    visible: Vec<u64>,
    capacity: usize,
}

impl ConnectionVisibilityBitset {
    /// Pre-allocates storage. `capacity` must equal `max_replicated_entities + 1`
    /// (matching `GlobalDirtyBitset::new(capacity, ...)`; slot 0 is the INVALID sentinel).
    pub fn new(capacity: usize) -> Self {
        let words = capacity.div_ceil(64).max(1);
        Self {
            visible: vec![0u64; words],
            capacity,
        }
    }

    /// Mark entity `idx` as visible (in scope and not paused).
    pub fn set(&mut self, idx: GlobalEntityIndex) {
        let i = idx.as_usize();
        if i == 0 || i >= self.capacity {
            return;
        }
        self.visible[i / 64] |= 1u64 << (i % 64);
    }

    /// Mark entity `idx` as not visible (out of scope or paused).
    pub fn clear(&mut self, idx: GlobalEntityIndex) {
        let i = idx.as_usize();
        if i == 0 || i >= self.capacity {
            return;
        }
        self.visible[i / 64] &= !(1u64 << (i % 64));
    }

    /// Returns `true` iff entity `idx` is currently visible.
    pub fn is_set(&self, idx: GlobalEntityIndex) -> bool {
        let i = idx.as_usize();
        if i == 0 || i >= self.capacity {
            return false;
        }
        self.visible[i / 64] & (1u64 << (i % 64)) != 0
    }

    /// Iterates entities that are both visible for this connection AND globally dirty.
    /// O(capacity / 64) — the hot path for per-user candidate selection in Phase 9.
    pub fn intersect_dirty<'a>(
        &'a self,
        global: &'a GlobalDirtyBitset,
    ) -> impl Iterator<Item = GlobalEntityIndex> + 'a {
        self.visible
            .iter()
            .zip(global.dirty_entity_words())
            .enumerate()
            .flat_map(|(word_idx, (vis_word, dirty_word))| {
                let combined = vis_word & dirty_word.load(Ordering::Relaxed);
                IntersectIter {
                    word: combined,
                    base: word_idx * 64,
                }
            })
    }
}

struct IntersectIter {
    word: u64,
    base: usize,
}

impl Iterator for IntersectIter {
    type Item = GlobalEntityIndex;

    fn next(&mut self) -> Option<Self::Item> {
        if self.word == 0 {
            return None;
        }
        let bit = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1;
        Some(GlobalEntityIndex((self.base + bit) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::update::global_dirty_bitset::GlobalDirtyBitset;
    use crate::world::update::global_entity_index::GlobalEntityIndex;

    #[test]
    fn set_and_is_set() {
        let mut vis = ConnectionVisibilityBitset::new(65);
        let idx = GlobalEntityIndex(1);
        assert!(!vis.is_set(idx));
        vis.set(idx);
        assert!(vis.is_set(idx));
    }

    #[test]
    fn clear_removes_bit() {
        let mut vis = ConnectionVisibilityBitset::new(65);  // needs mut for set/clear
        let idx = GlobalEntityIndex(5);
        vis.set(idx);
        vis.clear(idx);
        assert!(!vis.is_set(idx));
    }

    #[test]
    fn invalid_idx_ignored() {
        let mut vis = ConnectionVisibilityBitset::new(65);
        vis.set(GlobalEntityIndex::INVALID);
        assert!(!vis.is_set(GlobalEntityIndex::INVALID));
        // capacity-overflow index
        vis.set(GlobalEntityIndex(200));
        assert!(!vis.is_set(GlobalEntityIndex(200)));
    }

    #[test]
    fn intersect_dirty_yields_both_set() {
        let mut vis = ConnectionVisibilityBitset::new(130);
        let idx1 = GlobalEntityIndex(1);
        let idx2 = GlobalEntityIndex(70); // crosses 64-bit word boundary
        let idx3_visible_not_dirty = GlobalEntityIndex(2);

        vis.set(idx1);
        vis.set(idx2);
        vis.set(idx3_visible_not_dirty); // visible but NOT dirty → must be excluded

        let dirty = GlobalDirtyBitset::new(130, 2);
        dirty.increment(idx1, 0);
        dirty.increment(idx2, 0);
        // idx3 not incremented → not dirty

        let mut results: Vec<GlobalEntityIndex> = vis.intersect_dirty(&dirty).collect();
        results.sort_by_key(|i| i.0);
        assert_eq!(results, vec![idx1, idx2]);
    }

    #[test]
    fn intersect_dirty_invisible_entity_excluded() {
        let vis = ConnectionVisibilityBitset::new(65);
        let idx = GlobalEntityIndex(3);
        // NOT set in visibility → should not appear even if dirty
        let dirty = GlobalDirtyBitset::new(65, 1);
        dirty.increment(idx, 0);

        let results: Vec<GlobalEntityIndex> = vis.intersect_dirty(&dirty).collect();
        assert!(results.is_empty());
    }
}
