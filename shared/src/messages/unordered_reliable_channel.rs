use std::collections::VecDeque;
use naia_serde::{BitReader, BitWriter};

use crate::{Manifest, NetEntityHandleConverter, protocol::protocolize::Protocolize, sequence_less_than, types::MessageId};

use super::{
    channel_config::{ChannelIndex, ReliableSettings},
    message_channel::{MessageChannel, OutgoingReliableChannel},
};

pub struct UnorderedReliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    outgoing_channel: OutgoingReliableChannel<P>,
    oldest_received_message_id: MessageId,
    received_message_ids: VecDeque<(MessageId, bool)>,
    incoming_messages: Vec<P>,
}

impl<P: Protocolize, C: ChannelIndex> UnorderedReliableChannel<P, C> {
    pub fn new(channel_index: C, reliable_settings: &ReliableSettings) -> Self {
        Self {
            channel_index: channel_index.clone(),
            outgoing_channel: OutgoingReliableChannel::new(
                reliable_settings,
            ),
            oldest_received_message_id: 0,
            received_message_ids: VecDeque::new(),
            incoming_messages: Vec::new(),
        }
    }

    fn recv_message(&mut self, message_id: MessageId, message: P) {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_id has been instantiated
        // already if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_id, self.oldest_received_message_id) {
            // already moved sliding window past this message id
            return;
        }

        let mut index = 0;
        let mut found = false;

        loop {
            if index < self.received_message_ids.len() {
                if let Some((old_message_id, _)) = self.received_message_ids.get(index) {
                    if *old_message_id == message_id {
                        found = true;
                    }
                }

                if found {
                    let (_, old_message) = self.received_message_ids.get_mut(index).unwrap();
                    if *old_message == false {
                        *old_message = true;
                        self.incoming_messages.push(message);
                    } else {
                        // already received this message
                    }

                    break;
                }
            } else {
                let next_message_id = self.oldest_received_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.received_message_ids.push_back((next_message_id, true));
                    self.incoming_messages.push(message);
                    break;
                } else {
                    self.received_message_ids
                        .push_back((next_message_id, false));
                }
            }

            index += 1;
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> MessageChannel<P, C> for UnorderedReliableChannel<P, C> {
    fn send_message(&mut self, message: P) {
        return self.outgoing_channel.send_message(message);
    }

    fn collect_outgoing_messages(
        &mut self,
        rtt_millis: &f32,
    ) {
        return self
            .outgoing_channel
            .generate_messages(rtt_millis);
    }

    fn collect_incoming_messages(&mut self, incoming_messages: &mut Vec<(C, P)>) {
        for message in self.incoming_messages.drain(..) {
            incoming_messages.push((self.channel_index.clone(), message));
        }

        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.received_message_ids.front() {
                has_message = true;
            }
            if has_message {
                self.received_message_ids.pop_front();
                self.oldest_received_message_id = self.oldest_received_message_id.wrapping_add(1);
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
        return self.outgoing_channel.write_messages(converter, writer);
    }

    fn read_messages(&mut self, reader: &mut BitReader, manifest: &Manifest<P>, converter: &dyn NetEntityHandleConverter) {
        let id_w_msgs = self.outgoing_channel.read_messages(reader, manifest, converter);
        for (id, message) in id_w_msgs {
            self.recv_message(id, message);
        }
    }
}
