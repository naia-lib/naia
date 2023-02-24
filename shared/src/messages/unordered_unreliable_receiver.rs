use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, Serde, SerdeErr};

use crate::{
    messages::{message_channel::MessageChannelReceiver, message_kinds::MessageKinds},
    Message, NetEntityHandleConverter,
};

use super::message_channel::ChannelReceiver;

pub struct UnorderedUnreliableReceiver {
    incoming_messages: VecDeque<Box<dyn Message>>,
}

impl UnorderedUnreliableReceiver {
    pub fn new() -> Self {
        Self {
            incoming_messages: VecDeque::new(),
        }
    }

    fn read_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        // read payload
        message_kinds.read(reader, converter)
    }

    fn recv_message(&mut self, message: Box<dyn Message>) {
        self.incoming_messages.push_back(message);
    }
}

impl ChannelReceiver<Box<dyn Message>> for UnorderedUnreliableReceiver {
    fn receive_messages(&mut self) -> Vec<Box<dyn Message>> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for UnorderedUnreliableReceiver {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let channel_continue = bool::de(reader)?;
            if !channel_continue {
                break;
            }

            let message = self.read_message(message_kinds, converter, reader)?;
            self.recv_message(message);
        }

        Ok(())
    }
}
