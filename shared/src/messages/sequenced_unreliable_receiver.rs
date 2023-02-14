use std::mem;

use naia_serde::{BitReader, SerdeErr};

use crate::messages::message_kinds::MessageKinds;
use crate::{sequence_greater_than, types::MessageIndex};

use super::{
    indexed_message_reader::IndexedMessageReader,
    message_channel::{ChannelReader, ChannelReceiver},
};

pub struct SequencedUnreliableReceiver<P> {
    newest_received_message_index: Option<MessageIndex>,
    incoming_messages: Vec<P>,
}

impl<P> SequencedUnreliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            newest_received_message_index: None,
            incoming_messages: Vec::new(),
        }
    }

    pub fn buffer_message(&mut self, message_index: MessageIndex, message: P) {
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

impl<P: Send + Sync> ChannelReceiver<P> for SequencedUnreliableReceiver<P> {
    /// Read messages and add them to the buffer, discard messages that are older
    /// than the most recent received message
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        channel_reader: &dyn ChannelReader<P>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, channel_reader, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(id, message);
        }
        Ok(())
    }

    fn receive_messages(&mut self) -> Vec<P> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}
