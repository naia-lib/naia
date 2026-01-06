use std::hash::Hash;

pub trait InScopeEntities<E: Copy + Eq + Hash + Sync + Send> {
    fn has_entity(&self, entity: &E) -> bool;
}
