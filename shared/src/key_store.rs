use std::marker::PhantomData;

use std::collections::VecDeque;

use super::keys::NaiaKey;

/// Simple implementation of a store that manages a recycling pool of u16 keys
#[derive(Debug)]
pub struct KeyGenerator<K: NaiaKey> {
    recycled_local_keys: VecDeque<u16>,
    next_new_local_key: u16,
    phantom: PhantomData<K>,
}

impl<K: NaiaKey> KeyGenerator<K> {
    /// Create a new KeyStore
    pub fn new() -> Self {
        KeyGenerator {
            recycled_local_keys: VecDeque::new(),
            next_new_local_key: 0,
            phantom: PhantomData,
        }
    }

    /// Get a new, unused key
    pub fn generate(&mut self) -> K {
        if let Some(local_key) = self.recycled_local_keys.pop_front() {
            return K::from_u16(local_key);
        }

        let output = self.next_new_local_key;
        self.next_new_local_key += 1;
        return K::from_u16(output);
    }

    /// Recycle a used key, freeing it up
    pub fn recycle_key(&mut self, local_key: &K) {
        self.recycled_local_keys.push_back(local_key.to_u16());
    }
}