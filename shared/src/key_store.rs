
use super::LocalActorKey;

/// Simple implementation of a store that manages a recycling pool of u16 keys
#[derive(Debug)]
pub struct KeyGenerator {
    recycled_local_keys: Vec<LocalActorKey>,
    next_new_local_key: LocalActorKey,
}

impl KeyGenerator {
    /// Create a new KeyStore
    pub fn new() -> Self {
        KeyGenerator {
            recycled_local_keys: Vec::new(),
            next_new_local_key: 0,
        }
    }

    /// Get a new, unused key
    pub fn get_new_local_key(&mut self) -> u16 {
        if let Some(local_key) = self.recycled_local_keys.pop() {
            return local_key;
        }

        let output = self.next_new_local_key;
        self.next_new_local_key += 1;
        return output;
    }

    /// Recycle a used key, freeing it up
    pub fn recycle_key(&mut self, local_key: &u16) {
        self.recycled_local_keys.push(*local_key);
    }
}