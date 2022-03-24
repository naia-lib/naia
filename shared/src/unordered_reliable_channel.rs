use std::collections::VecDeque;

use super::{
    protocolize::Protocolize, types::MessageId,
    ChannelIndex, ReliableSettings, reliable_channel::ReliableChannel, sequence_less_than, reliable_channel::OutgoingReliableChannel
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct UnorderedReliableChannel<P: Protocolize, C: ChannelIndex> {
    channel_index: C,
    outgoing_channel: OutgoingReliableChannel<P, C>,
    oldest_received_message_id: MessageId,
    received_message_ids: VecDeque<(MessageId, bool)>,
    incoming_messages: Vec<P>,
}

impl<P: Protocolize, C: ChannelIndex> UnorderedReliableChannel<P, C> {
    pub fn new(channel_index: C, reliable_settings: &ReliableSettings) -> Self {
        Self {
            channel_index: channel_index.clone(),
            outgoing_channel: OutgoingReliableChannel::new(channel_index.clone(), reliable_settings),
            oldest_received_message_id: 0,
            received_message_ids: VecDeque::new(),
            incoming_messages: Vec::new(),
        }
    }
}

impl<P: Protocolize, C: ChannelIndex> ReliableChannel<P, C> for UnorderedReliableChannel<P, C> {

    fn outgoing(&mut self) -> &mut OutgoingReliableChannel<P, C> {
        return &mut self.outgoing_channel;
    }

    fn recv_message(&mut self, message_id: MessageId, message: P) {

        // moving from oldest incoming message to newest
        // compare existing slots and see if the message_id has been instantiated already
        // if it has, put the message into the slot
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
                    if !old_message {
                        *old_message = true;
                        self.incoming_messages.push(message);
                        break;
                    } else {
                        // already received this message
                    }
                }
            } else {
                let next_message_id = self.oldest_received_message_id.wrapping_add(index as u16);

                if next_message_id == message_id {
                    self.received_message_ids.push_back((next_message_id, true));
                    self.incoming_messages.push(message);
                    break;
                } else {
                    self.received_message_ids.push_back((next_message_id, false));
                }
            }

            index += 1;
        }
    }

    fn generate_incoming_messages(&mut self, incoming_messages: &mut VecDeque<(C, P)>) {

        for message in self.incoming_messages {
            incoming_messages.push_back((self.channel_index.clone(), message));
        }

        if self.incoming_messages.len() == 0 {
            self.incoming_messages.clear();
        } else {
            self.incoming_messages.clear();
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
}