use std::collections::VecDeque;

use crate::protocol::protocolize::Protocolize;
use crate::types::MessageId;

use super::{channel_config::ChannelIndex, message_channel::MessageChannel};

pub struct UnorderedUnreliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    outgoing_messages: VecDeque<P>,
    incoming_messages: VecDeque<P>,
}

impl<P: Protocolize, C: ChannelIndex> UnorderedUnreliableChannel<P, C> {
    pub fn new(channel_index: C) -> Self {
        Self {
            channel_index: channel_index.clone(),
            outgoing_messages: VecDeque::new(),
            incoming_messages: VecDeque::new(),
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> MessageChannel<P, C> for UnorderedUnreliableChannel<P, C> {
    fn send_message(&mut self, message: P) {
        self.incoming_messages.push_back(message);
    }

    fn recv_message(&mut self, _: MessageId, message: P) {
        self.outgoing_messages.push_back(message);
    }

    fn collect_outgoing_messages(&mut self, _: &f32, outgoing_messages: &mut VecDeque<(C, MessageId, P)>) {
        while let Some(message) = self.outgoing_messages.pop_front() {
            outgoing_messages.push_back((self.channel_index.clone(), 0, message));
        }
    }

    fn collect_incoming_messages(&mut self, incoming_messages: &mut VecDeque<(C, P)>) {
        while let Some(message) = self.incoming_messages.pop_front() {
            incoming_messages.push_back((self.channel_index.clone(), message));
        }
    }

    fn notify_message_delivered(&mut self, _: &MessageId) {
        // not necessary for an unreliable channel
    }
}
