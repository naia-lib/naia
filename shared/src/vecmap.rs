use std::{collections::HashMap, hash::Hash};

// This data structure is an ugly hack to get a HashMap that iterates in a fixed insertion order ...
// I could use IndexMap, but how much would that affect my eventual compile size?
// Probably should refactor this later to work correctly

#[derive(Clone)]
pub struct VecMap<K: Eq + Hash, V> {
    pub map: HashMap<K, V>,
    pub vec: Vec<K>,
}

impl<K: Eq + Hash + Clone, V> VecMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            vec: Vec::new(),
        }
    }

    pub fn dual_insert(&mut self, key: K, value: V) {
        self.map.insert(key.clone(), value);
        self.vec.push(key);
    }
}
