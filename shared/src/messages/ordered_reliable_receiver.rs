use std::collections::VecDeque;

use naia_serde::BitReader;

use crate::{types::MessageId, wrapping_number::sequence_less_than};

use super::{
    message_channel::{ChannelReader, ChannelReceiver},
    reliable_receiver::ReliableReceiver,
};

// OrderedReliableReceiver

pub struct OrderedReliableReceiver<P> {
    oldest_waiting_message_id: MessageId,
    waiting_incoming_messages: VecDeque<(MessageId, Option<P>)>,
}

impl<P> Default for OrderedReliableReceiver<P> {
    fn default() -> Self {
        Self {
            oldest_waiting_message_id: 0,
            waiting_incoming_messages: VecDeque::default(),
        }
    }
}

impl<P> OrderedReliableReceiver<P> {
    pub fn buffer_message(&mut self, message_id: MessageId, message: P) {
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

    pub fn receive_messages(&mut self) -> Vec<P> {
        let mut output = Vec::new();
        loop {
            let mut has_message = false;
            if let Some((_, Some(_))) = self.waiting_incoming_messages.front() {
                has_message = true;
            }
            if has_message {
                let (_, message_opt) = self.waiting_incoming_messages.pop_front().unwrap();
                let message = message_opt.unwrap();
                output.push(message);
                self.oldest_waiting_message_id = self.oldest_waiting_message_id.wrapping_add(1);
            } else {
                break;
            }
        }
        output
    }
}

impl<P: Send + Sync> ChannelReceiver<P> for OrderedReliableReceiver<P> {
    fn read_messages(&mut self, channel_reader: &dyn ChannelReader<P>, bit_reader: &mut BitReader) {
        let id_w_msgs = ReliableReceiver::read_incoming_messages(channel_reader, bit_reader);
        for (id, message) in id_w_msgs {
            self.buffer_message(id, message);
        }
    }

    fn receive_messages(&mut self) -> Vec<P> {
        self.receive_messages()
    }
}
