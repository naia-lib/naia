use std::{collections::VecDeque, marker::PhantomData, time::Duration};

use naia_socket_shared::Instant;

/// Simple implementation of a store that manages a recycling pool of u16 keys
pub struct KeyGenerator<K: From<u16> + Into<u16> + Copy> {
    recycled_local_keys: VecDeque<(u16, Instant)>,
    recycle_timeout: Duration,
    next_new_local_key: u16,
    phantom: PhantomData<K>,
}

impl<K: From<u16> + Into<u16> + Copy> KeyGenerator<K> {
    pub fn new(recycle_timeout: Duration) -> Self {
        Self {
            recycle_timeout,
            recycled_local_keys: VecDeque::new(),
            next_new_local_key: 0,
            phantom: PhantomData,
        }
    }
    /// Get a new, unused key
    pub fn generate(&mut self) -> K {
        let mut should_pop = false;
        if let Some((_, instant)) = self.recycled_local_keys.front() {
            if instant.elapsed() > self.recycle_timeout {
                should_pop = true;
            }
        }
        if should_pop {
            let (local_key, _) = self.recycled_local_keys.pop_front().unwrap();
            return K::from(local_key);
        }

        let output = self.next_new_local_key;
        self.next_new_local_key = self.next_new_local_key.wrapping_add(1);
        K::from(output)
    }

    /// Recycle a used key, freeing it up
    pub fn recycle_key(&mut self, local_key: &K) {
        let local_key_u16: u16 = Into::<u16>::into(*local_key);
        self.recycled_local_keys
            .push_back((local_key_u16, Instant::now()));
    }
}
