use std::{collections::VecDeque, mem};

use naia_serde::BitReader;

use super::{
    message_channel::{ChannelReader, ChannelReceiver},
    message_list_header::read,
};

pub struct UnorderedUnreliableReceiver<P> {
    incoming_messages: VecDeque<P>,
}

impl<P> UnorderedUnreliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            incoming_messages: VecDeque::new(),
        }
    }

    fn read_message(
        &mut self,
        channel_reader: &dyn ChannelReader<P>,
        bit_reader: &mut BitReader,
    ) -> P {
        // read payload

        channel_reader.read(bit_reader)
    }

    fn recv_message(&mut self, message: P) {
        self.incoming_messages.push_back(message);
    }
}

impl<P: Send + Sync> ChannelReceiver<P> for UnorderedUnreliableReceiver<P> {
    fn read_messages(&mut self, channel_reader: &dyn ChannelReader<P>, bit_reader: &mut BitReader) {
        let message_count = read(bit_reader);
        for _x in 0..message_count {
            let message = self.read_message(channel_reader, bit_reader);
            self.recv_message(message);
        }
    }

    fn receive_messages(&mut self) -> Vec<P> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}
