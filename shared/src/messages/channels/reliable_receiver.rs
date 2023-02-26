use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{
        channels::{
            indexed_message_reader::IndexedMessageReader,
            message_channel::{ChannelReceiver, MessageChannelReceiver},
        },
        message_kinds::MessageKinds,
    },
    sequence_less_than,
    types::MessageIndex,
    Message, NetEntityHandleConverter,
};

// Receiver Arranger Trait
pub trait ReceiverArranger<M>: Send + Sync {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, M)>,
        message_index: MessageIndex,
        message: M,
    );
}

// Reliable Receiver
pub struct ReliableReceiver<A: ReceiverArranger<M>, M> {
    oldest_received_message_index: MessageIndex,
    record: VecDeque<(MessageIndex, bool)>,
    incoming_messages: Vec<(MessageIndex, M)>,
    arranger: A,
}

impl<A: ReceiverArranger<M>, M> ReliableReceiver<A, M> {
    pub fn with_arranger(arranger: A) -> Self {
        Self {
            oldest_received_message_index: 0,
            record: VecDeque::default(),
            incoming_messages: Vec::default(),
            arranger,
        }
    }

    fn push_message(&mut self, message_index: MessageIndex, message: M) {
        self.arranger
            .process(&mut self.incoming_messages, message_index, message);
    }

    pub fn buffer_message(&mut self, message_index: MessageIndex, message: M) {
        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_index has been instantiated
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
            let mut should_push_message = false;
            if current_index < self.record.len() {
                if let Some((old_message_index, old_message)) = self.record.get_mut(current_index) {
                    if *old_message_index == message_index {
                        if !(*old_message) {
                            *old_message = true;
                            should_push_message = true;
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
                    should_push_message = true;
                } else {
                    self.record.push_back((next_message_index, false));
                    // keep filling up buffer
                }
            }

            if should_push_message {
                self.push_message(message_index, message);
                self.clear_old_messages();
                return;
            }

            current_index += 1;
        }
    }

    pub fn receive_messages(&mut self) -> Vec<(MessageIndex, M)> {
        // return buffer
        mem::take(&mut self.incoming_messages)
    }

    fn clear_old_messages(&mut self) {
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
    }
}

impl<A: ReceiverArranger<M>, M: Send + Sync> ChannelReceiver<M> for ReliableReceiver<A, M> {
    fn receive_messages(&mut self) -> Vec<M> {
        self.receive_messages()
            .drain(..)
            .map(|(_, message)| message)
            .collect()
    }
}

impl<A: ReceiverArranger<Box<dyn Message>>> MessageChannelReceiver
    for ReliableReceiver<A, Box<dyn Message>>
{
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
