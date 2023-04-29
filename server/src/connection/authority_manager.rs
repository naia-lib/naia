use std::{collections::HashSet, hash::Hash};

pub struct AuthorityManager<E: Copy + Eq + Hash + Send + Sync> {
    // Public Entities OWNED by the Connection
    public_entities: HashSet<E>,
}

impl<E: Copy + Eq + Hash + Send + Sync> AuthorityManager<E> {
    pub fn new() -> Self {
        Self {
            public_entities: HashSet::new(),
        }
    }

    pub fn add_public(&mut self, entity: &E) {
        self.public_entities.insert(*entity);
    }

    pub fn remove_public(&mut self, entity: &E) {
        self.public_entities.remove(entity);
    }

    pub fn is_public(&self, entity: &E) -> bool {
        self.public_entities.contains(entity)
    }
}
