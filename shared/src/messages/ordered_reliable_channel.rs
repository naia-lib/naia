use std::collections::VecDeque;

use naia_serde::{BitReader, BitWriter};

use crate::{
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    sequence_less_than,
    types::MessageId,
};

use super::{
    channel_config::{ChannelIndex, ReliableSettings},
    message_channel::MessageChannel,
    outgoing_reliable_channel::OutgoingReliableChannel,
};

pub struct OrderedReliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    outgoing_channel: OutgoingReliableChannel<P>,
    oldest_waiting_message_id: MessageId,
    waiting_incoming_messages: VecDeque<(MessageId, Option<P>)>,
}

impl<P: Protocolize, C: ChannelIndex> OrderedReliableChannel<P, C> {
    pub fn new(channel_index: C, reliable_settings: &ReliableSettings) -> Self {
        Self {
            channel_index: channel_index.clone(),
            outgoing_channel: OutgoingReliableChannel::new(reliable_settings),
            oldest_waiting_message_id: 0,
            waiting_incoming_messages: VecDeque::new(),
        }
    }

    pub fn recv_message(&mut self, message_id: MessageId, message: P) {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_id has been instantiated
        // already if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_id, self.oldest_waiting_message_id) {
            // already moved sliding window past this message id
            return;
        }

        let mut index = 0;
        let mut found = false;

        loop {
            if index < self.waiting_incoming_messages.len() {
                if let Some((old_message_id, _)) = self.waiting_incoming_messages.get(index) {
                    if *old_message_id == message_id {
                        found = true;
                    }
                }

                if found {
                    let (_, old_message) = self.waiting_incoming_messages.get_mut(index).unwrap();
                    if old_message.is_none() {
                        *old_message = Some(message);
                    } else {
                        // already received this message
                    }
                    break;
                }
            } else {
                let next_message_id = self.oldest_waiting_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.waiting_incoming_messages
                        .push_back((next_message_id, Some(message)));
                    break;
                } else {
                    self.waiting_incoming_messages
                        .push_back((next_message_id, None));
                }
            }

            index += 1;
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> MessageChannel<P, C> for OrderedReliableChannel<P, C> {
    fn send_message(&mut self, message: P) {
        return self.outgoing_channel.send_message(message);
    }

    fn collect_outgoing_messages(&mut self, rtt_millis: &f32) {
        return self.outgoing_channel.collect_outgoing_messages(rtt_millis);
    }

    fn collect_incoming_messages(&mut self, incoming_messages: &mut Vec<(C, P)>) {
        loop {
            let mut has_message = false;
            if let Some((_, Some(_))) = self.waiting_incoming_messages.front() {
                has_message = true;
            }
            if has_message {
                let (_, message_opt) = self.waiting_incoming_messages.pop_front().unwrap();
                let message = message_opt.unwrap();
                incoming_messages.push((self.channel_index.clone(), message));
                self.oldest_waiting_message_id = self.oldest_waiting_message_id.wrapping_add(1);
            } else {
                break;
            }
        }
    }

    fn notify_message_delivered(&mut self, message_id: &MessageId) {
        return self.outgoing_channel.notify_message_delivered(message_id);
    }

    fn has_outgoing_messages(&self) -> bool {
        return self.outgoing_channel.has_outgoing_messages();
    }

    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>> {
        return self.outgoing_channel.write_outgoing_messages(converter, writer);
    }

    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let id_w_msgs = self
            .outgoing_channel
            .read_incoming_messages(reader, manifest, converter);
        for (id, message) in id_w_msgs {
            self.recv_message(id, message);
        }
    }
}
