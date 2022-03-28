use std::{collections::VecDeque, mem};

use naia_serde::BitReader;

use crate::protocol::{
    entity_property::NetEntityHandleConverter, protocolize::Protocolize,
};

use super::{message_channel::ChannelReceiver, message_list_header::read};

pub struct UnorderedUnreliableReceiver<P: Protocolize> {
    incoming_messages: VecDeque<P>,
}

impl<P: Protocolize> UnorderedUnreliableReceiver<P> {
    pub fn new() -> Self {
        Self {
            incoming_messages: VecDeque::new(),
        }
    }

    fn read_message(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> P {
        // read payload
        let new_message = P::build(reader, converter);

        return new_message;
    }

    fn recv_message(&mut self, message: P) {
        self.incoming_messages.push_back(message);
    }
}

impl<P: Protocolize> ChannelReceiver<P> for UnorderedUnreliableReceiver<P> {
    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) {
        let message_count = read(reader);
        for _x in 0..message_count {
            let message = self.read_message(reader, converter);
            self.recv_message(message);
        }
    }

    fn receive_messages(&mut self) -> Vec<P> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}
