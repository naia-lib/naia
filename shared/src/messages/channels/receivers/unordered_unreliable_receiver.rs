use std::{collections::VecDeque, mem};

use naia_serde::{BitReader, Serde, SerdeErr};

use crate::{
    messages::{
        channels::receivers::channel_receiver::{ChannelReceiver, MessageChannelReceiver},
        message_kinds::MessageKinds,
    },
    LocalEntityAndGlobalEntityConverter, MessageContainer,
};

pub struct UnorderedUnreliableReceiver {
    incoming_messages: VecDeque<MessageContainer>,
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
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<MessageContainer, SerdeErr> {
        // read payload
        message_kinds.read(reader, converter)
    }

    fn recv_message(&mut self, message: MessageContainer) {
        self.incoming_messages.push_back(message);
    }
}

impl ChannelReceiver<MessageContainer> for UnorderedUnreliableReceiver {
    fn receive_messages(&mut self) -> Vec<MessageContainer> {
        Vec::from(mem::take(&mut self.incoming_messages))
    }
}

impl MessageChannelReceiver for UnorderedUnreliableReceiver {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
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
