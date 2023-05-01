// A Hashmap that can be queried by either key or value.

use std::{collections::HashMap, hash::Hash};

pub struct DoubleMap<
    K: Eq + PartialEq + Clone + Copy + Hash,
    V: Eq + PartialEq + Clone + Copy + Hash,
> {
    key_to_value: HashMap<K, V>,
    value_to_key: HashMap<V, K>,
}

impl<K: Eq + PartialEq + Clone + Copy + Hash, V: Eq + PartialEq + Clone + Copy + Hash>
    DoubleMap<K, V>
{
    pub fn new() -> Self {
        Self {
            key_to_value: HashMap::new(),
            value_to_key: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.key_to_value.insert(key, value);
        self.value_to_key.insert(value, key);
    }

    pub fn get_by_key(&self, key: &K) -> Option<&V> {
        self.key_to_value.get(key)
    }

    pub fn get_mut_by_key(&mut self, key: &K) -> Option<&mut V> {
        self.key_to_value.get_mut(key)
    }

    pub fn get_by_value(&self, value: &V) -> Option<&K> {
        self.value_to_key.get(value)
    }

    pub fn get_mut_by_value(&mut self, value: &V) -> Option<&mut K> {
        self.value_to_key.get_mut(value)
    }

    pub fn remove_by_key(&mut self, key: &K) -> Option<V> {
        let value = self.key_to_value.remove(key);
        if let Some(value) = value {
            self.value_to_key.remove(&value);
        }
        value
    }

    pub fn remove_by_value(&mut self, value: &V) -> Option<K> {
        let key = self.value_to_key.remove(value);
        if let Some(key) = key {
            self.key_to_value.remove(&key);
        }
        key
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.key_to_value.contains_key(key)
    }

    pub fn contains_value(&self, value: &V) -> bool {
        self.value_to_key.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.key_to_value.len()
    }

    pub fn is_empty(&self) -> bool {
        self.key_to_value.is_empty()
    }

    pub fn clear(&mut self) {
        self.key_to_value.clear();
        self.value_to_key.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.key_to_value.iter()
    }
}
