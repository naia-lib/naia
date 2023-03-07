use std::{collections::HashMap, net::SocketAddr};

use naia_shared::{MutChannelType, MutReceiver};

pub struct MutChannelData {
    receiver_map: HashMap<SocketAddr, MutReceiver>,
    diff_mask_length: u8,
}

impl MutChannelData {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            receiver_map: HashMap::new(),
            diff_mask_length,
        }
    }
}

impl MutChannelType for MutChannelData {
    fn new_receiver(&mut self, address_opt: &Option<SocketAddr>) -> Option<MutReceiver> {
        let address = address_opt.expect("cannot initialize receiver without address");
        if let Some(receiver) = self.receiver_map.get(&address) {
            Some(receiver.clone())
        } else {
            let receiver = MutReceiver::new(self.diff_mask_length);
            self.receiver_map.insert(address, receiver.clone());

            Some(receiver)
        }
    }

    fn send(&self, diff: u8) {
        for (_, receiver) in self.receiver_map.iter() {
            receiver.mutate(diff);
        }
    }
}
