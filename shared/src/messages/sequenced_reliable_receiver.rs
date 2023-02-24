use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{message_channel::MessageChannelReceiver, message_kinds::MessageKinds},
    sequence_less_than,
    types::MessageIndex,
    Message, NetEntityHandleConverter,
};

use super::{indexed_message_reader::IndexedMessageReader, message_channel::ChannelReceiver};

pub struct SequencedReliableReceiver {
    newest_received_message_index: MessageIndex,
    oldest_received_message_index: MessageIndex,
    record: VecDeque<(MessageIndex, bool)>,
    incoming_messages: Vec<(MessageIndex, Box<dyn Message>)>,
}

impl Default for SequencedReliableReceiver {
    fn default() -> Self {
        Self {
            newest_received_message_index: 0,
            oldest_received_message_index: 0,
            record: VecDeque::default(),
            incoming_messages: Vec::default(),
        }
    }
}

impl SequencedReliableReceiver {
    // Private methods

    pub fn buffer_message(&mut self, message_index: MessageIndex, message: Box<dyn Message>) {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_kind has been instantiated
        // already if it has, put the message into the slot
        // otherwise, keep track of what the last message id was
        // then add new empty slots at the end until getting to the incoming message id
        // then, once you're there, put the new message in

        if sequence_less_than(message_index, self.oldest_received_message_index) {
            // already moved sliding window past this message id
            return;
        }

        let mut current_index = 0;

        loop {
            if current_index < self.record.len() {
                if let Some((old_message_index, old_message)) = self.record.get_mut(current_index) {
                    if *old_message_index == message_index {
                        if !(*old_message) {
                            *old_message = true;
                            if !sequence_less_than(
                                *old_message_index,
                                self.newest_received_message_index,
                            ) {
                                self.newest_received_message_index = *old_message_index;
                                self.incoming_messages.push((*old_message_index, message));
                            }
                            return;
                        } else {
                            // already received this message
                            return;
                        }
                    }
                }
            } else {
                let next_message_index = self
                    .oldest_received_message_index
                    .wrapping_add(current_index as u16);

                if next_message_index == message_index {
                    self.record.push_back((next_message_index, true));
                    if !sequence_less_than(message_index, self.newest_received_message_index) {
                        self.newest_received_message_index = message_index;
                        self.incoming_messages.push((message_index, message));
                    }
                    return;
                } else {
                    self.record.push_back((next_message_index, false));
                    // keep filling up buffer
                    current_index += 1;
                    continue;
                }
            }

            current_index += 1;
        }
    }

    pub fn receive_messages(&mut self) -> Vec<(MessageIndex, Box<dyn Message>)> {
        // clear all received messages from record
        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.record.front() {
                has_message = true;
            }
            if has_message {
                self.record.pop_front();
                self.oldest_received_message_index =
                    self.oldest_received_message_index.wrapping_add(1);
            } else {
                break;
            }
        }

        // return buffer
        mem::take(&mut self.incoming_messages)
    }
}

impl ChannelReceiver<Box<dyn Message>> for SequencedReliableReceiver {
    fn receive_messages(&mut self) -> Vec<Box<dyn Message>> {
        let mut output: Vec<Box<dyn Message>> = Vec::new();
        for (_, message) in self.receive_messages() {
            output.push(message);
        }
        output
    }
}

impl MessageChannelReceiver for SequencedReliableReceiver {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, converter, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(id, message);
        }
        Ok(())
    }
}
