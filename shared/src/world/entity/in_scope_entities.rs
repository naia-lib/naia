use std::hash::Hash;

/// Query interface for determining whether a given entity is currently within a receiver's scope.
pub trait InScopeEntities<E: Copy + Eq + Hash + Sync + Send> {
    /// Returns `true` if `entity` is currently tracked as in-scope.
    fn has_entity(&self, entity: &E) -> bool;
}
