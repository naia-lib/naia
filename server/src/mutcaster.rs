use std::{sync::{Arc, RwLock}, net::SocketAddr, collections::VecDeque};

use indexmap::IndexMap;

pub struct Mutcaster;

impl Mutcaster {
    pub fn new_channel() -> (MutSender, MutReceiverBuilder) {

        let channel = MutChannel::new();

        let sender = channel.new_sender();

        let builder = MutReceiverBuilder::new(&channel);

        (sender, builder)
    }
}

// MutChannel
#[derive(Clone)]
pub struct MutChannel {
    data: Arc<RwLock<MutChannelData>>,
}

impl MutChannel {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(MutChannelData::new())),
        }
    }

    pub fn new_sender(&self) -> MutSender {

        return MutSender::new(self);
    }

    pub fn new_receiver(&self, addr: &SocketAddr) -> Option<MutReceiver> {
        if let Ok(mut data) = self.data.as_ref().write() {
            return data.new_receiver(addr);
        }
        return None;
    }

    pub fn send(&self, diff: u8) -> bool {
        if let Ok(data) = self.data.as_ref().read() {
            data.send(diff);
            return true;
        }
        return false;
    }
}

struct MutChannelData {
    queue_map: IndexMap<SocketAddr, MutReceiver>,
}

impl MutChannelData {
    pub fn new() -> Self {
        Self {
            queue_map: IndexMap::new(),
        }
    }

    pub fn new_receiver(&mut self, addr: &SocketAddr) -> Option<MutReceiver> {
        if self.queue_map.contains_key(addr) {
            return None;
        }

        let q = MutReceiver::new();
        self.queue_map.insert(*addr, q.clone());

        return Some(q);
    }

    pub fn send(&self, diff: u8) {
        for (_, receiver) in self.queue_map.iter() {
            receiver.enqueue(diff);
        }
    }
}

// MutReceiver
#[derive(Clone)]
pub struct MutReceiver {
    queue: Arc<RwLock<VecDeque<u8>>>,
}

impl MutReceiver {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub fn enqueue(&self, diff: u8) {
        if let Ok(mut deque) = self.queue.as_ref().write() {
            deque.push_back(diff);
        }
    }

    pub fn recv(&self) -> Option<u8> {
        if let Ok(mut deque) = self.queue.as_ref().write() {
            return deque.pop_front();
        }
        return None;
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

    pub fn send(&self, diff: u8) -> bool {
        return self.channel.send(diff);
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

    pub fn build(&self, addr: &SocketAddr) -> Option<MutReceiver> {
        return self.channel.new_receiver(addr);
    }
}