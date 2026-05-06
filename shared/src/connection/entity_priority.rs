use std::{collections::HashMap, hash::Hash};

/// Stored per-entity-bundle accumulator state. One of these per (entity, layer),
/// where layer is either the sender-wide "global" layer or a per-connection
/// "per-user" layer.
#[derive(Clone, Debug, Default)]
pub(crate) struct EntityPriorityData {
    /// Running priority accumulator; reset to 0 on send.
    pub(crate) accumulated: f32,
    /// User-set persistent gain override. `None` means the default (1.0) applies.
    pub(crate) gain_override: Option<f32>,
    /// Sender's game tick at last successful send. Telemetry only; not used in
    /// priority calculation (the accumulator itself encodes staleness).
    pub(crate) last_sent_tick: Option<u32>,
}

/// Read-only view of an entity's priority state in one priority layer
/// (global OR per-user). Acquired via the corresponding `*_priority()` method
/// on `WorldServer`, `Client`, or their Bevy-adapter equivalents.
pub struct EntityPriorityRef<'a, E: Copy + Eq + Hash> {
    pub(crate) state: Option<&'a EntityPriorityData>,
    pub(crate) entity: E,
}

impl<'a, E: Copy + Eq + Hash> EntityPriorityRef<'a, E> {
    /// Construct an empty read-only handle (no backing entry). Reads return
    /// defaults: `accumulated() == 0.0`, `gain() == None`. Used when the
    /// caller wants a handle for an entity whose layer doesn't yet exist.
    pub fn empty(entity: E) -> Self {
        Self {
            state: None,
            entity,
        }
    }

    pub fn entity(&self) -> E {
        self.entity
    }

    /// Current accumulated priority value for this layer. Higher = more urgent.
    /// Returns `0.0` if this entity has no accumulator entry yet.
    pub fn accumulated(&self) -> f32 {
        self.state.map(|s| s.accumulated).unwrap_or(0.0)
    }

    /// Current per-tick gain override for this layer. `None` means the default
    /// (1.0) applies.
    pub fn gain(&self) -> Option<f32> {
        self.state.and_then(|s| s.gain_override)
    }

    pub fn is_overridden(&self) -> bool {
        self.gain().is_some()
    }
}

/// Mutable handle for reading and setting an entity's priority in one priority
/// layer. Lazy-creates a state entry on first write so set-and-forget works
/// even before the entity enters scope for that user.
///
/// Returned by `global_entity_priority_mut` / `user_entity_priority_mut` on the
/// server, `entity_priority_mut` on the client, and their Bevy-adapter
/// passthroughs.
pub struct EntityPriorityMut<'a, E: Copy + Eq + Hash> {
    pub(crate) entries: &'a mut HashMap<E, EntityPriorityData>,
    pub(crate) entity: E,
}

impl<'a, E: Copy + Eq + Hash> EntityPriorityMut<'a, E> {
    // --- Reads (mirror Ref) ---

    pub fn entity(&self) -> E {
        self.entity
    }

    pub fn accumulated(&self) -> f32 {
        self.entries
            .get(&self.entity)
            .map(|s| s.accumulated)
            .unwrap_or(0.0)
    }

    pub fn gain(&self) -> Option<f32> {
        self.entries
            .get(&self.entity)
            .and_then(|s| s.gain_override)
    }

    pub fn is_overridden(&self) -> bool {
        self.gain().is_some()
    }

    // --- Writes ---

    /// Set a persistent per-tick gain override for this layer. Stays in effect
    /// until `reset()` or another `set_gain()` call. Lazy-creates the entry.
    pub fn set_gain(&mut self, gain: f32) -> &mut Self {
        self.entries
            .entry(self.entity)
            .or_default()
            .gain_override = Some(gain);
        self
    }

    /// One-shot additive boost to the accumulator. Does not change gain.
    /// Multiple calls in one tick sum additively. Lazy-creates the entry.
    /// Persists across ticks until the entity is sent (then reset to 0).
    pub fn boost_once(&mut self, amount: f32) -> &mut Self {
        self.entries
            .entry(self.entity)
            .or_default()
            .accumulated += amount;
        self
    }

    /// Clear the gain override — return to default (1.0). Does NOT clear the
    /// accumulator value itself, and does NOT remove the entry.
    pub fn reset(&mut self) -> &mut Self {
        if let Some(data) = self.entries.get_mut(&self.entity) {
            data.gain_override = None;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> HashMap<u32, EntityPriorityData> {
        HashMap::new()
    }

    #[test]
    fn set_gain_lazy_creates_entry() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        assert_eq!(m.gain(), None);
        m.set_gain(5.0);
        assert_eq!(m.gain(), Some(5.0));
        assert!(m.is_overridden());
    }

    #[test]
    fn set_gain_then_reset_returns_to_default() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.set_gain(5.0);
        m.reset();
        assert_eq!(m.gain(), None);
        assert!(!m.is_overridden());
        // Entry retained.
        assert!(entries.contains_key(&7u32));
    }

    #[test]
    fn boost_once_is_additive_and_preserves_gain() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.set_gain(3.0);
        m.boost_once(10.0);
        m.boost_once(5.0);
        assert_eq!(m.accumulated(), 15.0);
        assert_eq!(m.gain(), Some(3.0));
    }

    #[test]
    fn boost_once_lazy_creates_entry() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.boost_once(4.0);
        assert_eq!(m.accumulated(), 4.0);
        // No gain override set by boost.
        assert_eq!(m.gain(), None);
    }

    #[test]
    fn reset_on_absent_entry_is_noop() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.reset();
        assert!(!entries.contains_key(&7u32));
    }

    // B-BDD-4: set_gain(5.0) then reset() → default applied;
    // is_overridden() == false; entry still exists.
    #[test]
    fn b_bdd_4_set_gain_then_reset_yields_default() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.set_gain(5.0);
        assert!(m.is_overridden());
        m.reset();
        assert_eq!(m.gain(), None);
        assert!(!m.is_overridden());
        assert!(entries.contains_key(&7u32));
    }

    // B-BDD-5 (write-side): boost_once(100.0) bumps accumulator +100 immediately
    // without mutating gain. Reset-on-send is a drain-path concern and lives in
    // the send loop — not testable at this unit level.
    #[test]
    fn b_bdd_5_boost_once_does_not_mutate_gain() {
        let mut entries = fresh();
        let mut m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        m.set_gain(3.0);
        m.boost_once(100.0);
        assert_eq!(m.accumulated(), 100.0);
        assert_eq!(m.gain(), Some(3.0));
    }

    // B-BDD-6: set_gain(5.0) persists across subsequent mutations (boost_once,
    // additional reads). At this layer, "next tick" is simply any state between
    // mutations — the entry's gain_override field carries forward until reset.
    #[test]
    fn b_bdd_6_set_gain_persists_across_mutations() {
        let mut entries = fresh();
        {
            let mut m = EntityPriorityMut {
                entries: &mut entries,
                entity: 7u32,
            };
            m.set_gain(5.0);
            m.boost_once(10.0);
        }
        // Re-acquire handle later (simulates next tick's access) — gain persists.
        let m = EntityPriorityMut {
            entries: &mut entries,
            entity: 7u32,
        };
        assert_eq!(m.gain(), Some(5.0));
        assert_eq!(m.accumulated(), 10.0);
    }

    // Ref-side read API mirrors Mut-side read API — important for `*_priority()`
    // (read-only) returning handles consistent with mutable handles.
    #[test]
    fn ref_reads_match_mut_reads() {
        let mut entries = fresh();
        {
            let mut m = EntityPriorityMut {
                entries: &mut entries,
                entity: 7u32,
            };
            m.set_gain(2.0);
            m.boost_once(7.0);
        }
        let r = EntityPriorityRef {
            state: entries.get(&7u32),
            entity: 7u32,
        };
        assert_eq!(r.gain(), Some(2.0));
        assert_eq!(r.accumulated(), 7.0);
        assert!(r.is_overridden());
    }

    // Absent-entry read-only handle returns defaults (no entry needed for reads).
    #[test]
    fn empty_ref_reads_defaults() {
        let r = EntityPriorityRef::<u32>::empty(42);
        assert_eq!(r.entity(), 42);
        assert_eq!(r.gain(), None);
        assert_eq!(r.accumulated(), 0.0);
        assert!(!r.is_overridden());
    }
}
