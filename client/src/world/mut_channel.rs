use std::net::SocketAddr;

use naia_shared::{CachedComponentUpdate, MutChannelType, MutReceiver};

pub struct MutChannelData {
    receiver: MutReceiver,
}

impl MutChannelData {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            receiver: MutReceiver::new(diff_mask_length),
        }
    }
}

impl MutChannelType for MutChannelData {
    fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
        if address_opt.is_some() {
            panic!(
                "should not initialize client MutReceiver with an address (there is only 1 server)"
            );
        }
        Some(self.receiver.clone())
    }

    fn send(&self, diff: u8) {
        self.receiver.mutate(diff);
    }

    fn get_cached_update(&self, _diff_mask_key: u64) -> Option<CachedComponentUpdate> {
        None
    }

    fn set_cached_update(&self, _diff_mask_key: u64, _update: CachedComponentUpdate) {
        // no-op: client does not maintain a send-side cached update store
    }

    fn clear_cached_updates(&self) {
        // no-op: client does not maintain a send-side cached update store
    }
}
