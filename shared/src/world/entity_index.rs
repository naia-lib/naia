/// Per-user dense index for a `GlobalEntity` known to a `UserDiffHandler`.
///
/// Phase 8.1 Stage A introduces this newtype as the in-process key for
/// dirty-set tracking and (eventually) packed mask storage. Each user's
/// `UserDiffHandler` issues one `LocalEntityIndex` per `GlobalEntity` it
/// observes via [`crate::KeyGenerator`] at its `u32` width, recycling on
/// `deregister_component` once the entity has no remaining components in
/// the user's receiver map. `u32` instead of `u16` because the index space
/// is per-user (16K indices isn't always enough at multi-thousand entity
/// scopes).
///
/// **Wire-format independent.** This index never crosses the wire; it's
/// purely an in-memory shortcut so dirty queues and (Stage B) bit-vec
/// membership tests can use Vec-indexed operations instead of HashMap
/// probes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LocalEntityIndex(pub u32);

impl From<u32> for LocalEntityIndex {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<LocalEntityIndex> for u32 {
    fn from(value: LocalEntityIndex) -> Self {
        value.0
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::KeyGenerator;

    /// `LocalEntityIndex` is issued from the shared [`KeyGenerator`] at its u32
    /// width. These pin the properties this index depends on — sequential
    /// issue, a capacity hint that tracks the high-water mark, and a quarantine
    /// that genuinely withholds a freed index.
    #[test]
    fn generates_sequential_keys() {
        let mut g: KeyGenerator<LocalEntityIndex, u32> =
            KeyGenerator::new(Duration::from_secs(60));
        assert_eq!(g.generate().0, 0);
        assert_eq!(g.generate().0, 1);
        assert_eq!(g.generate().0, 2);
    }

    #[test]
    fn capacity_hint_matches_next_new_key() {
        let mut g: KeyGenerator<LocalEntityIndex, u32> =
            KeyGenerator::new(Duration::from_secs(60));
        assert_eq!(g.capacity_hint(), 0);
        let _ = g.generate();
        let _ = g.generate();
        assert_eq!(g.capacity_hint(), 2);
    }

    #[test]
    fn recycle_keeps_key_quarantined_until_timeout() {
        let mut g: KeyGenerator<LocalEntityIndex, u32> =
            KeyGenerator::new(Duration::from_secs(60));
        let k = g.generate();
        g.recycle_key(&k);
        let next = g.generate();
        assert_ne!(next.0, k.0, "recycled key should not return before timeout");
    }

    #[test]
    fn recycle_returns_after_timeout() {
        let mut g: KeyGenerator<LocalEntityIndex, u32> =
            KeyGenerator::new(Duration::from_millis(0));
        let k = g.generate();
        g.recycle_key(&k);
        // Spin briefly to ensure elapsed > 0
        std::thread::sleep(Duration::from_millis(2));
        let next = g.generate();
        assert_eq!(next.0, k.0, "recycled key should be reused after timeout");
    }
}
