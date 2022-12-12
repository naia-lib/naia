use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, SerdeErr};

use crate::{message_list_header, sequence_greater_than, types::MessageId};

use super::{
    message_channel::{ChannelReader, ChannelReceiver},
    reliable_receiver::ReliableReceiver,
};

pub struct SequencedUnreliableReceiver<P> {
    most_recent_received_message_id: Option<MessageId>,
    incoming_messages: VecDeque<P>,
}

impl<P> SequencedUnreliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            most_recent_received_message_id: None,
            incoming_messages: VecDeque::new(),
        }
    }


    fn recv_message(&mut self, message: P) {
        self.incoming_messages.push_back(message);
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
        let message_count = message_list_header::read(reader)?;

        for _x in 0..message_count {
            let (message_id, message) = ReliableReceiver::read_incoming_message(
                channel_reader, reader, &self.most_recent_received_message_id)?;

            // only process the message if it is the most recent one
            if let Some(most_recent_id) = self.most_recent_received_message_id {
                if sequence_greater_than(message_id, most_recent_id) {
                    self.recv_message(message);
                    self.most_recent_received_message_id = Some(message_id);
                }
            } else {
                self.most_recent_received_message_id = Some(message_id);
            }
        }
        Ok(())
    }

    fn receive_messages(&mut self) -> Vec<P> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}
