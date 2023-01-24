use std::mem;

use naia_serde::{BitReader, SerdeErr};

use crate::{sequence_greater_than, types::MessageId};

use super::{
    message_channel::{ChannelReader, ChannelReceiver},
    indexed_message_reader::IndexedMessageReader,
};

pub struct SequencedUnreliableReceiver<P> {
    newest_received_message_id: Option<MessageId>,
    incoming_messages: Vec<P>,
}

impl<P> SequencedUnreliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            newest_received_message_id: None,
            incoming_messages: Vec::new(),
        }
    }

    pub fn buffer_message(&mut self, message_id: MessageId, message: P) {
        if let Some(most_recent_id) = self.newest_received_message_id {
            if sequence_greater_than(message_id, most_recent_id) {
                self.incoming_messages.push(message);
                self.newest_received_message_id = Some(message_id);
            }
        } else {
            self.incoming_messages.push(message);
            self.newest_received_message_id = Some(message_id);
        }
    }
}

impl<P: Send + Sync> ChannelReceiver<P> for SequencedUnreliableReceiver<P> {
    /// Read messages and add them to the buffer, discard messages that are older
    /// than the most recent received message
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
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}
