use std::{collections::HashMap, hash::Hash};

use crate::connection::entity_priority::{EntityPriorityData, EntityPriorityMut, EntityPriorityRef};

/// Sender-wide priority layer. One instance lives on `WorldServer`.
/// Entries are evicted on entity despawn.
///
/// Combined multiplicatively with each `UserPriorityState` entry at sort time:
/// `effective_gain = global.gain.unwrap_or(1.0) × user.gain.unwrap_or(1.0)`.
pub struct GlobalPriorityState<E: Copy + Eq + Hash> {
    entries: HashMap<E, EntityPriorityData>,
}

impl<E: Copy + Eq + Hash> GlobalPriorityState<E> {
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
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get_ref(&self, entity: E) -> EntityPriorityRef<'_, E> {
        EntityPriorityRef {
            state: self.entries.get(&entity),
            entity,
        }
    }

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
}

impl<E: Copy + Eq + Hash> Default for UserPriorityState<E> {
    fn default() -> Self {
        Self::new()
    }
}
