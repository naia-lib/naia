use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{MutChannelType, MutReceiver};

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
}

impl MutChannelData {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            receiver_index: HashMap::new(),
            receivers: Vec::new(),
            diff_mask_length,
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
        // Wire-cache invalidation is now handled per-entity in GlobalDiffHandler
        // at the start of each send cycle (C.7.C Option B). No per-channel clear needed.
    }
}
