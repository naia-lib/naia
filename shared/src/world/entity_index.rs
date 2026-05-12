use std::{collections::VecDeque, time::Duration};

use naia_socket_shared::Instant;

/// Per-user dense index for a `GlobalEntity` known to a `UserDiffHandler`.
///
/// Phase 8.1 Stage A introduces this newtype as the in-process key for
/// dirty-set tracking and (eventually) packed mask storage. Each user's
/// `UserDiffHandler` issues one `EntityIndex` per `GlobalEntity` it
/// observes via [`crate::KeyGenerator32`], recycling on
/// `deregister_component` once the entity has no remaining components in
/// the user's receiver map. `u32` instead of `u16` because the index space
/// is per-user (16K indices isn't always enough at multi-thousand entity
/// scopes) and because `KeyGenerator`'s u16 wrap-around bug was already
/// noted as a follow-up item.
///
/// **Wire-format independent.** This index never crosses the wire; it's
/// purely an in-memory shortcut so dirty queues and (Stage B) bit-vec
/// membership tests can use Vec-indexed operations instead of HashMap
/// probes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityIndex(pub u32);

impl From<u32> for EntityIndex {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<EntityIndex> for u32 {
    fn from(value: EntityIndex) -> Self {
        value.0
    }
}

/// u32 variant of [`crate::KeyGenerator`] — same recycling semantics, wider
/// key space. Forked rather than generic-ified to keep the existing u16
/// generator's call sites untouched.
#[derive(Clone)]
pub struct KeyGenerator32<K: From<u32> + Into<u32> + Copy> {
    recycling_keys: VecDeque<(u32, Instant)>,
    recycled_keys: VecDeque<u32>,
    recycle_timeout: Duration,
    next_new_key: u32,
    phantom: std::marker::PhantomData<K>,
}

impl<K: From<u32> + Into<u32> + Copy> KeyGenerator32<K> {
    /// Creates a generator that quarantines recycled keys for at least `recycle_timeout` before reissuing them.
    pub fn new(recycle_timeout: Duration) -> Self {
        Self {
            recycle_timeout,
            recycling_keys: VecDeque::new(),
            recycled_keys: VecDeque::new(),
            next_new_key: 0,
            phantom: std::marker::PhantomData,
        }
    }

    /// Issues the next available key, preferring recycled keys whose quarantine period has elapsed.
    pub fn generate(&mut self) -> K {
        let now = Instant::now();
        loop {
            let Some((_, instant)) = self.recycling_keys.front() else {
                break;
            };
            if instant.elapsed(&now) < self.recycle_timeout {
                break;
            }
            let (key, _) = self.recycling_keys.pop_front().unwrap();
            self.recycled_keys.push_back(key);
        }
        if let Some(key) = self.recycled_keys.pop_front() {
            return K::from(key);
        }
        let output = self.next_new_key;
        self.next_new_key = self.next_new_key.wrapping_add(1);
        K::from(output)
    }

    /// Marks `key` as available for reuse after the configured quarantine period expires.
    pub fn recycle_key(&mut self, key: &K) {
        let key_u32: u32 = (*key).into();
        self.recycling_keys.push_back((key_u32, Instant::now()));
    }

    /// Highest index ever issued + 1. Used to size bit-vec storage in Stage B.
    pub fn capacity_hint(&self) -> u32 {
        self.next_new_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_sequential_keys() {
        let mut g: KeyGenerator32<EntityIndex> = KeyGenerator32::new(Duration::from_secs(60));
        assert_eq!(g.generate().0, 0);
        assert_eq!(g.generate().0, 1);
        assert_eq!(g.generate().0, 2);
    }

    #[test]
    fn capacity_hint_matches_next_new_key() {
        let mut g: KeyGenerator32<EntityIndex> = KeyGenerator32::new(Duration::from_secs(60));
        assert_eq!(g.capacity_hint(), 0);
        let _ = g.generate();
        let _ = g.generate();
        assert_eq!(g.capacity_hint(), 2);
    }

    #[test]
    fn recycle_keeps_key_quarantined_until_timeout() {
        let mut g: KeyGenerator32<EntityIndex> = KeyGenerator32::new(Duration::from_secs(60));
        let k = g.generate();
        g.recycle_key(&k);
        let next = g.generate();
        assert_ne!(next.0, k.0, "recycled key should not return before timeout");
    }

    #[test]
    fn recycle_returns_after_timeout() {
        let mut g: KeyGenerator32<EntityIndex> = KeyGenerator32::new(Duration::from_millis(0));
        let k = g.generate();
        g.recycle_key(&k);
        // Spin briefly to ensure elapsed > 0
        std::thread::sleep(Duration::from_millis(2));
        let next = g.generate();
        assert_eq!(next.0, k.0, "recycled key should be reused after timeout");
    }
}
