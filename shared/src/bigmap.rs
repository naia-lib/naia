use std::{
    collections::{
        hash_map::{Iter, IterMut},
        HashMap,
    },
    hash::Hash,
    iter::Map,
    marker::PhantomData,
};

/// Key type for [`BigMap`]: must be convertible to/from `u64`.
pub trait BigMapKey: Clone + Copy + Eq + PartialEq + Hash {
    /// Converts the key to its `u64` representation.
    fn to_u64(&self) -> u64;
    /// Reconstructs a key from its `u64` representation.
    fn from_u64(value: u64) -> Self;
}

/// Auto-incrementing dense map that generates monotone `u64`-backed typed keys.
pub struct BigMap<K: BigMapKey, V> {
    inner: HashMap<u64, V>,
    current_index: u64,
    phantom_k: PhantomData<K>,
}

impl<K: BigMapKey, V> Default for BigMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: BigMapKey, V> BigMap<K, V> {
    /// Creates an empty `BigMap`.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            current_index: 0,
            phantom_k: PhantomData,
        }
    }

    /// Returns a reference to the value for `key`, or `None` if not present.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(&key.to_u64())
    }

    /// Returns a mutable reference to the value for `key`, or `None` if not present.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(&key.to_u64())
    }

    /// Inserts a value and returns its newly generated key. Panics on `u64` overflow.
    pub fn insert(&mut self, value: V) -> K {
        // [entity-replication-11] GlobalEntity rollover is a terminal error
        if self.current_index == u64::MAX {
            panic!(
                "BigMap counter overflow: cannot allocate new key (current_index = u64::MAX). \
                 This is a terminal error per entity-replication-11 spec."
            );
        }

        let old_index = self.current_index;
        self.current_index = self.current_index.wrapping_add(1);

        self.inner.insert(old_index, value);

        K::from_u64(old_index)
    }

    /// Removes and returns the value for `key`, or `None` if not present.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(&key.to_u64())
    }

    /// Returns `true` if the map contains an entry for `key`.
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(&key.to_u64())
    }

    /// Returns an iterator over `(K, &V)` pairs in arbitrary order.
    #[allow(clippy::type_complexity)]
    pub fn iter<'a>(&'a self) -> Map<Iter<'a, u64, V>, fn((&'a u64, &'a V)) -> (K, &'a V)> {
        self
            .inner
            .iter()
            .map(|(key, value)| (K::from_u64(*key), value))
    }

    /// Returns a mutable iterator over `(K, &mut V)` pairs in arbitrary order.
    #[allow(clippy::type_complexity)]
    pub fn iter_mut<'a>(
        &'a mut self,
    ) -> Map<IterMut<'a, u64, V>, fn((&'a u64, &'a mut V)) -> (K, &'a mut V)> {
        self
            .inner
            .iter_mut()
            .map(|(key, value)| (K::from_u64(*key), value))
    }

    /// Returns the number of entries currently in the map.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[cfg(feature = "test_utils")]
    #[doc(hidden)]
    pub fn set_current_index_for_test(&mut self, index: u64) {
        self.current_index = index;
    }
}
