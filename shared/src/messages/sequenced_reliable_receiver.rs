use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, SerdeErr};

use crate::{sequence_less_than, types::MessageId};

use super::{
    indexed_message_reader::IndexedMessageReader,
    message_channel::{ChannelReader, ChannelReceiver},
};

pub struct SequencedReliableReceiver<P> {
    newest_received_message_id: MessageId,
    oldest_received_message_id: MessageId,
    record: VecDeque<(MessageId, bool)>,
    incoming_messages: Vec<(MessageId, P)>,
}

impl<P> Default for SequencedReliableReceiver<P> {
    fn default() -> Self {
        Self {
            newest_received_message_id: 0,
            oldest_received_message_id: 0,
            record: VecDeque::default(),
            incoming_messages: Vec::default(),
        }
    }
}

impl<P> SequencedReliableReceiver<P> {
    // Private methods

    pub fn buffer_message(&mut self, message_id: MessageId, message: P) {
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

        loop {
            if index < self.record.len() {
                if let Some((old_message_id, old_message)) = self.record.get_mut(index) {
                    if *old_message_id == message_id {
                        if !(*old_message) {
                            *old_message = true;
                            if !sequence_less_than(*old_message_id, self.newest_received_message_id)
                            {
                                self.newest_received_message_id = *old_message_id;
                                self.incoming_messages.push((*old_message_id, message));
                            }
                            return;
                        } else {
                            // already received this message
                            return;
                        }
                    }
                }
            } else {
                let next_message_id = self.oldest_received_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.record.push_back((next_message_id, true));
                    if !sequence_less_than(message_id, self.newest_received_message_id) {
                        self.newest_received_message_id = message_id;
                        self.incoming_messages.push((message_id, message));
                    }
                    return;
                } else {
                    self.record.push_back((next_message_id, false));
                    // keep filling up buffer
                    index += 1;
                    continue;
                }
            }

            index += 1;
        }
    }

    pub fn receive_messages(&mut self) -> Vec<(MessageId, P)> {
        // clear all received messages from record
        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.record.front() {
                has_message = true;
            }
            if has_message {
                self.record.pop_front();
                self.oldest_received_message_id = self.oldest_received_message_id.wrapping_add(1);
            } else {
                break;
            }
        }

        // return buffer
        mem::take(&mut self.incoming_messages)
    }
}

impl<P: Send + Sync> ChannelReceiver<P> for SequencedReliableReceiver<P> {
    fn read_messages(
        &mut self,
        channel_reader: &dyn ChannelReader<P>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(channel_reader, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(id, message);
        }
        Ok(())
    }

    fn receive_messages(&mut self) -> Vec<P> {
        let mut output: Vec<P> = Vec::new();
        for (_, message) in self.receive_messages() {
            output.push(message);
        }
        output
    }
}
