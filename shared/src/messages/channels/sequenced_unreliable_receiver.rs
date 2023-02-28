use std::mem;

use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{
        channels::{
            indexed_message_reader::IndexedMessageReader,
            message_channel::{ChannelReceiver, MessageChannelReceiver},
        },
        message_kinds::MessageKinds,
    },
    sequence_greater_than,
    types::MessageIndex,
    MessageContainer, NetEntityHandleConverter,
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

    pub fn buffer_message(&mut self, message_index: MessageIndex, message: MessageContainer) {
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
    fn receive_messages(&mut self) -> Vec<MessageContainer> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for SequencedUnreliableReceiver {
    /// Read messages and add them to the buffer, discard messages that are older
    /// than the most recent received message
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
