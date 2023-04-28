use std::{collections::VecDeque, marker::PhantomData, time::Duration};

use naia_socket_shared::Instant;

/// Simple implementation of a store that manages a recycling pool of u16 keys
pub struct KeyGenerator<K: From<u16> + Into<u16> + Copy> {
    recycling_keys: VecDeque<(u16, Instant)>,
    recycled_keys: VecDeque<u16>,
    recycle_timeout: Duration,
    next_new_key: u16,
    phantom: PhantomData<K>,
}

impl<K: From<u16> + Into<u16> + Copy> KeyGenerator<K> {
    pub fn new(recycle_timeout: Duration) -> Self {
        Self {
            recycle_timeout,
            recycling_keys: VecDeque::new(),
            recycled_keys: VecDeque::new(),
            next_new_key: 0,
            phantom: PhantomData,
        }
    }
    /// Get a new, unused key
    pub fn generate(&mut self) -> K {
        // Check whether we can recycle any keys
        loop {
            let Some((_, instant)) = self.recycling_keys.front() else {
                break;
            };
            if instant.elapsed() < self.recycle_timeout {
                break;
            }
            let (key, _) = self.recycling_keys.pop_front().unwrap();
            self.recycled_keys.push_back(key);
        }

        // Check whether we can return a recycled key
        if self.recycled_keys.len() > 0 {
            let key = self.recycled_keys.pop_front().unwrap();
            return K::from(key);
        }

        // Create a new key
        let output = self.next_new_key;
        self.next_new_key = self.next_new_key.wrapping_add(1);
        K::from(output)
    }

    /// Recycle a used key, freeing it up
    pub fn recycle_key(&mut self, key: &K) {
        let key_u16: u16 = Into::<u16>::into(*key);
        self.recycling_keys.push_back((key_u16, Instant::now()));
    }
}
