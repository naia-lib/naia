use std::{collections::HashMap, net::SocketAddr, sync::RwLock};

use naia_shared::{CachedComponentUpdate, MutChannelType, MutReceiver};

/// Server-side mut channel data.
///
/// Phase 8.1 Stage D (2026-04-25) — split storage into a HashMap (for
/// idempotent `new_receiver` lookup) plus a `Vec<MutReceiver>` (for the
/// hot-path `send` walk). The `Vec` is contiguous memory, so the
/// per-mutation fan-out is a tight cache-friendly loop instead of a
/// HashMap iteration. Concurrency: send is single-threaded today
/// (`world_server::send_all_packets`); if parallel-per-user packet
/// build is ever wanted, swap in an SPSC queue per slot.
pub struct MutChannelData {
    /// Address → index into `receivers`. Used only by `new_receiver` for
    /// idempotent lookup; never read on the send hot path.
    receiver_index: HashMap<SocketAddr, usize>,
    /// Hot-path fan-out targets. Walked once per mutation.
    receivers: Vec<MutReceiver>,
    diff_mask_length: u8,
    /// Pre-serialized update cache, keyed by `DiffMask::as_key()`.
    /// Invalidated automatically on every property mutation via `send()`.
    cached_updates: RwLock<HashMap<u64, CachedComponentUpdate>>,
}

impl MutChannelData {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            receiver_index: HashMap::new(),
            receivers: Vec::new(),
            diff_mask_length,
            cached_updates: RwLock::new(HashMap::new()),
        }
    }
}

impl MutChannelType for MutChannelData {
    fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
        let address = address_opt.expect("cannot initialize receiver without address");
        if let Some(&idx) = self.receiver_index.get(&address) {
            return Some(self.receivers[idx].clone());
        }
        let receiver = MutReceiver::new(self.diff_mask_length);
        let idx = self.receivers.len();
        self.receivers.push(receiver.clone());
        self.receiver_index.insert(address, idx);
        Some(receiver)
    }

    fn send(&self, property_index: u8) {
        for receiver in &self.receivers {
            receiver.mutate(property_index);
        }
        self.clear_cached_updates();
    }

    fn get_cached_update(&self, diff_mask_key: u64) -> Option<CachedComponentUpdate> {
        self.cached_updates.read().ok()?.get(&diff_mask_key).copied()
    }

    fn set_cached_update(&self, diff_mask_key: u64, update: CachedComponentUpdate) {
        if let Ok(mut cache) = self.cached_updates.write() {
            cache.insert(diff_mask_key, update);
        }
    }

    fn clear_cached_updates(&self) {
        if let Ok(mut cache) = self.cached_updates.write() {
            cache.clear();
        }
    }
}

#[cfg(test)]
mod cached_update_store_tests {
    use super::*;
    use naia_shared::CachedComponentUpdate;

    fn make_update(bit_count: u32) -> CachedComponentUpdate {
        let mut bytes = [0u8; 64];
        bytes[0] = 0xAB;
        CachedComponentUpdate { bytes, bit_count }
    }

    #[test]
    fn store_and_retrieve() {
        let ch = MutChannelData::new(1);
        let update = make_update(8);
        ch.set_cached_update(0x01, update);
        let got = ch.get_cached_update(0x01).expect("should return stored update");
        assert_eq!(got.bit_count, 8);
        assert_eq!(got.bytes[0], 0xAB);
    }

    #[test]
    fn send_clears_cache() {
        let ch = MutChannelData::new(1);
        ch.set_cached_update(0x01, make_update(8));
        assert!(ch.get_cached_update(0x01).is_some());
        // send() fans out to receivers (none registered here) then clears the cache
        ch.send(0);
        assert!(ch.get_cached_update(0x01).is_none(), "cache must be cleared after send");
    }

    #[test]
    fn cache_survives_without_mutation() {
        let ch = MutChannelData::new(1);
        ch.set_cached_update(0xFF, make_update(16));
        // No mutation — cache entry remains valid across multiple reads
        assert!(ch.get_cached_update(0xFF).is_some());
        assert!(ch.get_cached_update(0xFF).is_some());
    }

    #[test]
    fn missing_key_returns_none() {
        let ch = MutChannelData::new(1);
        assert!(ch.get_cached_update(0x99).is_none());
    }

    #[test]
    fn multiple_keys_coexist() {
        let ch = MutChannelData::new(1);
        ch.set_cached_update(0x01, make_update(8));
        ch.set_cached_update(0x03, make_update(12));
        assert_eq!(ch.get_cached_update(0x01).unwrap().bit_count, 8);
        assert_eq!(ch.get_cached_update(0x03).unwrap().bit_count, 12);
    }

    #[test]
    fn send_clears_all_keys() {
        let ch = MutChannelData::new(1);
        ch.set_cached_update(0x01, make_update(8));
        ch.set_cached_update(0x03, make_update(12));
        ch.send(0);
        assert!(ch.get_cached_update(0x01).is_none());
        assert!(ch.get_cached_update(0x03).is_none());
    }
}
