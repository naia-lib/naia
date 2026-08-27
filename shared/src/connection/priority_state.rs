use std::{collections::HashMap, hash::Hash};

use crate::connection::entity_priority::{
    EntityPriorityData, EntityPriorityMut, EntityPriorityRef,
};
use crate::world::entity::global_entity::GlobalEntity;

/// Per-tick hook the send loop calls to (a) advance accumulators for dirty
/// entity bundles, and (b) reset accumulators for bundles that fully drained.
///
/// The hook is keyed by `GlobalEntity` so it can be implemented inside
/// `naia-shared` without leaking the server's world-entity type. Implementations
/// translate to their internal entity representation as needed.
///
/// See PRIORITY_ACCUMULATOR_PLAN.md III.7 — the canonical accumulator rules.
pub trait OutgoingPriorityHook {
    /// Advance the per-user accumulator for `entity` by its effective gain
    /// (`global.gain × user.gain`, defaults 1.0) and return the new accumulated
    /// value. The send loop sorts dirty entity bundles by this value descending
    /// to drive the k-way merge against the bandwidth budget.
    fn advance(&mut self, entity: &GlobalEntity) -> f32;

    /// Reset the per-user accumulator for `entity` after its update bundle has
    /// been fully drained into the wire this tick. Stamps `last_sent_tick`.
    fn reset_after_send(&mut self, entity: &GlobalEntity, current_tick: u32);
}

/// Sender-wide priority layer. One instance lives on `WorldServer`.
/// Entries are evicted on entity despawn.
///
/// Combined multiplicatively with each `UserPriorityState` entry at sort time:
/// `effective_gain = global.gain.unwrap_or(1.0) × user.gain.unwrap_or(1.0)`.
#[derive(Clone)]
pub struct GlobalPriorityState<E: Copy + Eq + Hash> {
    entries: HashMap<E, EntityPriorityData>,
}

impl<E: Copy + Eq + Hash> GlobalPriorityState<E> {
    /// Creates an empty `GlobalPriorityState`.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Read-only handle (lazy — returns `None` if no entry exists).
    pub fn get_ref(&self, entity: E) -> EntityPriorityRef<'_, E> {
        EntityPriorityRef {
            state: self.entries.get(&entity),
            entity,
        }
    }

    /// Mutable handle; lazy entry creation deferred to first write.
    pub fn get_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        EntityPriorityMut {
            entries: &mut self.entries,
            entity,
        }
    }

    /// Evict this entity's global entry. Called from `WorldServer::despawn_entity`.
    pub fn on_despawn(&mut self, entity: &E) {
        self.entries.remove(entity);
    }

    /// Read-only gain lookup. `None` if no override is in effect (default 1.0
    /// applies). Used by the send-time priority sort to compute effective gain.
    pub fn gain_override(&self, entity: &E) -> Option<f32> {
        self.entries.get(entity).and_then(|d| d.gain_override)
    }
}

impl<E: Copy + Eq + Hash> Default for GlobalPriorityState<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-connection priority layer. One instance lives on each user `Connection`
/// (and on the client's single `Connection`).
/// Entries are evicted on scope exit for that user.
pub struct UserPriorityState<E: Copy + Eq + Hash> {
    entries: HashMap<E, EntityPriorityData>,
}

impl<E: Copy + Eq + Hash> UserPriorityState<E> {
    /// Creates an empty `UserPriorityState`.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Returns a read-only priority handle for `entity` in this user's layer.
    pub fn get_ref(&self, entity: E) -> EntityPriorityRef<'_, E> {
        EntityPriorityRef {
            state: self.entries.get(&entity),
            entity,
        }
    }

    /// Returns a mutable priority handle for `entity` in this user's layer.
    pub fn get_mut(&mut self, entity: E) -> EntityPriorityMut<'_, E> {
        EntityPriorityMut {
            entries: &mut self.entries,
            entity,
        }
    }

    /// Evict this entity's per-user entry. Called on scope exit or connection drop.
    pub fn on_scope_exit(&mut self, entity: &E) {
        self.entries.remove(entity);
    }

    /// Read-only gain lookup for this user layer.
    pub fn gain_override(&self, entity: &E) -> Option<f32> {
        self.entries.get(entity).and_then(|d| d.gain_override)
    }

    /// Per-tick advance hook used by the send-side k-way merge.
    /// Adds `gain` to the entity's accumulator (lazy-creating the entry) and
    /// returns the new accumulated value.
    ///
    /// This is the canonical "accumulator += effective_gain per tick" rule
    /// from PRIORITY_ACCUMULATOR_PLAN.md III.7.1.
    pub fn advance(&mut self, entity: E, gain: f32) -> f32 {
        let entry = self.entries.entry(entity).or_default();
        entry.accumulated += gain;
        entry.accumulated
    }

    /// Read accumulator without advancing.
    pub fn accumulated(&self, entity: &E) -> f32 {
        self.entries
            .get(entity)
            .map(|d| d.accumulated)
            .unwrap_or(0.0)
    }

    /// task #13 (naia-server pipelined coord→send priority publish): merge this
    /// **per-tick coord staging** layer INTO `target` (the live `send` layer),
    /// preserving `target`'s accumulators, then clear self.
    ///
    /// For each staged entity:
    /// - if its gain was EXPLICITLY written this tick (`gain_dirty` — set_gain or
    ///   reset, NOT a boost-only touch), copy `gain_override` to `target` (this
    ///   replays both an explicit set AND a `reset()`-to-default);
    /// - always ADD the staged boost delta (`accumulated`) to `target`'s live
    ///   accumulator (one-shot — the staging is cleared, so boosts apply once).
    ///
    /// This reproduces, at the send preamble, exactly what the resident borrow
    /// API does by writing the live map directly between `receive` and `send` —
    /// byte-identical by construction (the accumulator advance/reset stays
    /// entirely send-side; eviction parity holds because the staging never
    /// persists across ticks). See MISSION_PIPELINE_API_BOUNDARY task #13.
    pub fn drain_merge_into(&mut self, target: &mut UserPriorityState<E>) {
        for (entity, data) in self.entries.drain() {
            let t = target.entries.entry(entity).or_default();
            if data.gain_dirty {
                t.gain_override = data.gain_override;
            }
            t.accumulated += data.accumulated;
        }
    }

    /// `true` if this layer has no entries (used to skip empty staging drains).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reset the accumulator to 0 and stamp `last_sent_tick`. Called for each
    /// entity whose update bundle fully drained in the current send cycle —
    /// the canonical reset-on-send rule from III.7.5.
    pub fn reset_after_send(&mut self, entity: &E, current_tick: u32) {
        if let Some(data) = self.entries.get_mut(entity) {
            data.accumulated = 0.0;
            data.last_sent_tick = Some(current_tick);
        }
    }
}

impl<E: Copy + Eq + Hash> Default for UserPriorityState<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_on_despawn_evicts_entry() {
        let mut s = GlobalPriorityState::<u32>::new();
        s.get_mut(7).set_gain(5.0);
        assert_eq!(s.get_ref(7).gain(), Some(5.0));
        s.on_despawn(&7);
        assert_eq!(s.get_ref(7).gain(), None);
    }

    #[test]
    fn user_on_scope_exit_evicts_entry() {
        let mut s = UserPriorityState::<u32>::new();
        s.get_mut(7).boost_once(3.0);
        assert_eq!(s.get_ref(7).accumulated(), 3.0);
        s.on_scope_exit(&7);
        assert_eq!(s.get_ref(7).accumulated(), 0.0);
    }

    #[test]
    fn global_eviction_preserves_other_entries() {
        let mut s = GlobalPriorityState::<u32>::new();
        s.get_mut(7).set_gain(5.0);
        s.get_mut(9).set_gain(2.0);
        s.on_despawn(&7);
        assert_eq!(s.get_ref(7).gain(), None);
        assert_eq!(s.get_ref(9).gain(), Some(2.0));
    }

    // ── task #13: drain_merge_into (the byte-critical coord-staging → send merge)
    //
    // Each test models one tick: a fresh `staging` layer (the per-tick coord
    // writes) merged into a `send` layer (the live, persisted send-side state),
    // and asserts the result equals what resident's direct write to `send` would
    // have produced.

    #[test]
    fn merge_set_gain_applies_to_send() {
        let mut send = UserPriorityState::<u32>::new();
        let mut staging = UserPriorityState::<u32>::new();
        staging.get_mut(7).set_gain(5.0);
        staging.drain_merge_into(&mut send);
        assert_eq!(send.get_ref(7).gain(), Some(5.0));
        assert!(staging.is_empty(), "staging is drained");
    }

    #[test]
    fn merge_boost_only_preserves_send_accumulator_and_gain() {
        // send already has a gain + accumulator from prior ticks.
        let mut send = UserPriorityState::<u32>::new();
        send.get_mut(7).set_gain(3.0);
        send.advance(7, 10.0); // live accumulator = 10
                               // This tick: a boost-only touch (no gain write).
        let mut staging = UserPriorityState::<u32>::new();
        staging.get_mut(7).boost_once(4.0);
        staging.drain_merge_into(&mut send);
        // Gain must be UNTOUCHED (boost-only must not clobber it to None).
        assert_eq!(send.get_ref(7).gain(), Some(3.0));
        // Boost is ADDED to the live accumulator (10 + 4), not replaced.
        assert_eq!(send.accumulated(&7), 14.0);
    }

    #[test]
    fn merge_reset_clears_send_gain_in_a_later_tick() {
        // tick 1: set_gain → send gain = 5.
        let mut send = UserPriorityState::<u32>::new();
        {
            let mut t1 = UserPriorityState::<u32>::new();
            t1.get_mut(7).set_gain(5.0);
            t1.drain_merge_into(&mut send);
        }
        assert_eq!(send.get_ref(7).gain(), Some(5.0));
        // tick 2 (staging empty/cleared): reset() must still reach send.gain → None.
        let mut t2 = UserPriorityState::<u32>::new();
        t2.get_mut(7).reset();
        t2.drain_merge_into(&mut send);
        assert_eq!(
            send.get_ref(7).gain(),
            None,
            "reset clears the persisted send gain"
        );
    }

    #[test]
    fn merge_set_gain_then_boost_same_tick() {
        let mut send = UserPriorityState::<u32>::new();
        let mut staging = UserPriorityState::<u32>::new();
        staging.get_mut(7).set_gain(2.0).boost_once(8.0);
        staging.drain_merge_into(&mut send);
        assert_eq!(send.get_ref(7).gain(), Some(2.0));
        assert_eq!(send.accumulated(&7), 8.0);
    }

    #[test]
    fn merge_does_not_touch_unstaged_send_entries() {
        let mut send = UserPriorityState::<u32>::new();
        send.get_mut(9).set_gain(1.5);
        let mut staging = UserPriorityState::<u32>::new();
        staging.get_mut(7).set_gain(5.0);
        staging.drain_merge_into(&mut send);
        assert_eq!(
            send.get_ref(9).gain(),
            Some(1.5),
            "untouched entry preserved"
        );
        assert_eq!(send.get_ref(7).gain(), Some(5.0));
    }
}
