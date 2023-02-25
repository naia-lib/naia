use std::collections::VecDeque;

use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{
        channels::{
            indexed_message_reader::IndexedMessageReader,
            message_channel::{ChannelReceiver, MessageChannelReceiver},
        },
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    wrapping_number::sequence_less_than,
    Message, NetEntityHandleConverter,
};

// OrderedReliableReceiver

pub struct OrderedReliableReceiver {
    oldest_received_message_index: MessageIndex,
    incoming_messages: VecDeque<(MessageIndex, Option<Box<dyn Message>>)>,
}

impl Default for OrderedReliableReceiver {
    fn default() -> Self {
        Self {
            oldest_received_message_index: 0,
            incoming_messages: VecDeque::default(),
        }
    }
}

impl OrderedReliableReceiver {
    pub fn buffer_message(&mut self, message_index: MessageIndex, message: Box<dyn Message>) {
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

        let mut index = 0;
        let mut found = false;

        loop {
            if index < self.incoming_messages.len() {
                if let Some((old_message_index, _)) = self.incoming_messages.get(index) {
                    if *old_message_index == message_index {
                        found = true;
                    }
                }

                if found {
                    let (_, old_message) = self.incoming_messages.get_mut(index).unwrap();
                    if old_message.is_none() {
                        *old_message = Some(message);
                    } else {
                        // already received this message
                    }
                    break;
                }
            } else {
                let next_message_index = self
                    .oldest_received_message_index
                    .wrapping_add(index as u16);

                if next_message_index == message_index {
                    self.incoming_messages
                        .push_back((next_message_index, Some(message)));
                    break;
                } else {
                    self.incoming_messages.push_back((next_message_index, None));
                }
            }

            index += 1;
        }
    }

    pub fn receive_messages(&mut self) -> Vec<Box<dyn Message>> {
        let mut output = Vec::new();
        loop {
            let mut has_message = false;
            if let Some((_, Some(_))) = self.incoming_messages.front() {
                has_message = true;

                // IF this is a FRAGMENT, check whether all subsequent fragments have been received
                // before merging them all together and adding to outgoing list
                todo!(); connor
            }
            if has_message {
                let (_, message_opt) = self.incoming_messages.pop_front().unwrap();
                let message = message_opt.unwrap();
                output.push(message);
                self.oldest_received_message_index =
                    self.oldest_received_message_index.wrapping_add(1);
            } else {
                break;
            }
        }
        output
    }
}

impl ChannelReceiver<Box<dyn Message>> for OrderedReliableReceiver {
    fn receive_messages(&mut self) -> Vec<Box<dyn Message>> {
        self.receive_messages()
    }
}

impl MessageChannelReceiver for OrderedReliableReceiver {
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
