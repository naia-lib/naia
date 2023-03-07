use std::{
    hash::Hash,
    net::SocketAddr,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use crate::{DiffMask, GlobalWorldManagerType, PropertyMutate};

pub trait MutChannelType: Send + Sync {
    fn new_receiver(&mut self, address: &Option<SocketAddr>) -> Option<MutReceiver>;
    fn send(&self, diff: u8);
}

// MutChannel
#[derive(Clone)]
pub struct MutChannel {
    data: Arc<RwLock<dyn MutChannelType>>,
}

impl MutChannel {
    pub fn new_channel<E: Copy + Eq + Hash>(
        global_world_manager: &dyn GlobalWorldManagerType<E>,
        diff_mask_length: u8,
    ) -> (MutSender, MutReceiverBuilder) {
        let channel = Self {
            data: global_world_manager.new_mut_channel(diff_mask_length),
        };

        let sender = channel.new_sender();

        let builder = MutReceiverBuilder::new(&channel);

        (sender, builder)
    }

    pub fn new_sender(&self) -> MutSender {
        MutSender::new(self)
    }

    pub fn new_receiver(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        if let Ok(mut data) = self.data.as_ref().write() {
            return data.new_receiver(address);
        }
        None
    }

    pub fn send(&self, diff: u8) -> bool {
        if let Ok(data) = self.data.as_ref().read() {
            data.send(diff);
            return true;
        }
        false
    }
}

// MutReceiver
#[derive(Clone)]
pub struct MutReceiver {
    mask: Arc<RwLock<DiffMask>>,
}

impl MutReceiver {
    pub fn new(diff_mask_length: u8) -> Self {
        Self {
            mask: Arc::new(RwLock::new(DiffMask::new(diff_mask_length))),
        }
    }

    pub fn mask(&self) -> Option<RwLockReadGuard<DiffMask>> {
        self.mask.as_ref().read().ok()
    }

    pub fn diff_mask_is_clear(&self) -> bool {
        if let Ok(mask) = self.mask.as_ref().read() {
            return mask.is_clear();
        }
        true
    }

    pub fn mutate(&self, diff: u8) {
        if let Ok(mut mask) = self.mask.as_ref().write() {
            mask.set_bit(diff, true);
        }
    }

    pub fn or_mask(&self, other_mask: &DiffMask) {
        if let Ok(mut mask) = self.mask.as_ref().write() {
            mask.or(other_mask);
        }
    }

    pub fn clear_mask(&self) {
        if let Ok(mut mask) = self.mask.as_ref().write() {
            mask.clear();
        }
    }
}

// MutSender
#[derive(Clone)]
pub struct MutSender {
    channel: MutChannel,
}

impl MutSender {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }
}

impl PropertyMutate for MutSender {
    fn mutate(&mut self, property_index: u8) {
        self.channel.send(property_index);
    }
}

// MutReceiverBuilder
pub struct MutReceiverBuilder {
    channel: MutChannel,
}

impl MutReceiverBuilder {
    pub fn new(channel: &MutChannel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }

    pub fn build(&self, address: &Option<SocketAddr>) -> Option<MutReceiver> {
        self.channel.new_receiver(address)
    }
}
