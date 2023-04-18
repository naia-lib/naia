use std::mem;

use naia_serde::{BitReader, SerdeErr};

use crate::world::remote::entity_waitlist::EntityWaitlist;
use crate::{
    messages::{
        channels::receivers::{
            channel_receiver::{ChannelReceiver, MessageChannelReceiver},
            indexed_message_reader::IndexedMessageReader,
        },
        message_kinds::MessageKinds,
    },
    sequence_greater_than,
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverter, MessageContainer,
};

pub struct SequencedUnreliableReceiver {
    newest_received_message_index: Option<MessageIndex>,
    incoming_messages: Vec<MessageContainer>,
}

impl SequencedUnreliableReceiver {
    pub fn new() -> Self {
        Self {
            newest_received_message_index: None,
            incoming_messages: Vec::new(),
        }
    }

    pub fn buffer_message(
        &mut self,
        entity_waitlist: &mut EntityWaitlist,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        // use entity_waitlist
        todo!();

        if let Some(most_recent_id) = self.newest_received_message_index {
            if sequence_greater_than(message_index, most_recent_id) {
                self.incoming_messages.push(message);
                self.newest_received_message_index = Some(message_index);
            }
        } else {
            self.incoming_messages.push(message);
            self.newest_received_message_index = Some(message_index);
        }
    }
}

impl ChannelReceiver<MessageContainer> for SequencedUnreliableReceiver {
    fn receive_messages(&mut self, entity_waitlist: &mut EntityWaitlist) -> Vec<MessageContainer> {
        // use entity_waitlist
        todo!();

        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for SequencedUnreliableReceiver {
    /// Read messages and add them to the buffer, discard messages that are older
    /// than the most recent received message
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, converter, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(entity_waitlist, id, message);
        }
        Ok(())
    }
}
