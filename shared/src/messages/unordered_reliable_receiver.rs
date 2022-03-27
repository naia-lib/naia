use std::{collections::VecDeque, mem};

use naia_serde::BitReader;

use crate::{
    protocol::{
        entity_property::NetEntityHandleConverter, manifest::Manifest, protocolize::Protocolize,
    },
    sequence_less_than,
    types::MessageId,
};

use super::{
    message_channel::ChannelReceiver,
    reliable_receiver::ReliableReceiver,
};

pub struct UnorderedReliableReceiver<P: Protocolize> {
    oldest_waiting_message_id: MessageId,
    waiting_incoming_messages: VecDeque<(MessageId, bool)>,
    ready_incoming_messages: Vec<P>,
}

impl<P: Protocolize> UnorderedReliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            oldest_waiting_message_id: 0,
            waiting_incoming_messages: VecDeque::new(),
            ready_incoming_messages: Vec::new(),
        }
    }

    // Private methods

    fn recv_message(&mut self, message_id: MessageId, message: P) {
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
                    if *old_message == false {
                        *old_message = true;
                        self.ready_incoming_messages.push(message);
                    } else {
                        // already received this message
                    }

                    break;
                }
            } else {
                let next_message_id = self.oldest_waiting_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.waiting_incoming_messages
                        .push_back((next_message_id, true));
                    self.ready_incoming_messages.push(message);
                    break;
                } else {
                    self.waiting_incoming_messages
                        .push_back((next_message_id, false));
                }
            }

            index += 1;
        }
    }

    fn clear_sent_messages(&mut self) {
        loop {
            let mut has_message = false;
            if let Some((_, true)) = self.waiting_incoming_messages.front() {
                has_message = true;
            }
            if has_message {
                self.waiting_incoming_messages.pop_front();
                self.oldest_waiting_message_id = self.oldest_waiting_message_id.wrapping_add(1);
            } else {
                break;
            }
        }
    }
}

impl<P: Protocolize> ChannelReceiver<P> for UnorderedReliableReceiver<P> {
    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let id_w_msgs = ReliableReceiver::read_incoming_messages(reader, manifest, converter);
        for (id, message) in id_w_msgs {
            self.recv_message(id, message);
        }
    }

    fn collect_incoming_messages(&mut self) -> Vec<P> {
        self.clear_sent_messages();

        mem::take(&mut self.ready_incoming_messages)
    }
}
