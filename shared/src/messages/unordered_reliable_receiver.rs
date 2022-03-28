use std::{collections::VecDeque, mem};

use naia_serde::BitReader;

use crate::{sequence_less_than, types::MessageId};

use super::{
    message_channel::{ChannelReader, ChannelReceiver},
    reliable_receiver::ReliableReceiver,
};

pub struct UnorderedReliableReceiverRecord {
    oldest_received_message_id: MessageId,
    received_messages: VecDeque<(MessageId, bool)>,
}

impl UnorderedReliableReceiverRecord {
    pub fn new() -> Self {
        Self {
            oldest_received_message_id: 0,
            received_messages: VecDeque::new(),
        }
    }

    // Private methods

    pub fn should_receive_message(&mut self, message_id: MessageId) -> bool {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_id has been instantiated
        // already if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_id, self.oldest_received_message_id) {
            // already moved sliding window past this message id
            return false;
        }

        let mut index = 0;

        loop {
            if index < self.received_messages.len() {
                if let Some((old_message_id, old_message)) = self.received_messages.get_mut(index) {
                    if *old_message_id == message_id {
                        if *old_message == false {
                            *old_message = true;
                            return true;
                        } else {
                            // already received this message
                            return false;
                        }
                    }
                }
            } else {
                let next_message_id = self.oldest_received_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.received_messages.push_back((next_message_id, true));
                    return true;
                } else {
                    self.received_messages.push_back((next_message_id, false));
                    // keep filling up buffer
                    continue;
                }
            }

            index += 1;
        }
    }

    pub fn clear_sent_messages(&mut self) {
        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.received_messages.front() {
                has_message = true;
            }
            if has_message {
                self.received_messages.pop_front();
                self.oldest_received_message_id = self.oldest_received_message_id.wrapping_add(1);
            } else {
                break;
            }
        }
    }
}

pub struct UnorderedReliableReceiver<P> {
    record: UnorderedReliableReceiverRecord,
    ready_incoming_messages: Vec<P>,
}

impl<P> UnorderedReliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            record: UnorderedReliableReceiverRecord::new(),
            ready_incoming_messages: Vec::new(),
        }
    }
}

impl<P> ChannelReceiver<P> for UnorderedReliableReceiver<P> {
    fn read_messages(&mut self, channel_reader: &dyn ChannelReader<P>, bit_reader: &mut BitReader) {
        let id_w_msgs = ReliableReceiver::read_incoming_messages(channel_reader, bit_reader);
        for (id, message) in id_w_msgs {
            if self.record.should_receive_message(id) {
                self.ready_incoming_messages.push(message);
            }
        }
    }

    fn receive_messages(&mut self) -> Vec<P> {
        self.record.clear_sent_messages();

        mem::take(&mut self.ready_incoming_messages)
    }
}
