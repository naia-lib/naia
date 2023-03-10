use std::net::SocketAddr;

use naia_shared::{MutChannelType, MutReceiver};

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
        return Some(self.receiver.clone());
    }

    fn send(&self, diff: u8) {
        self.receiver.mutate(diff);
    }
}
