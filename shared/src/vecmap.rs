use std::{hash::Hash, collections::HashMap, slice::Iter};

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

    pub fn insert(&mut self, key: K, value: V) {
        self.map.insert(key.clone(), value);
        self.vec.push(key);
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        return self.map.get(key);
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        return self.map.get_mut(key);
    }

    pub fn iter(&self) -> Iter<K> {
        return self.vec.iter();
    }

    pub fn iter_mut(&mut self) -> Iter<K> {
        return self.vec.iter();
    }
}