use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{
        channels::receivers::{
            channel_receiver::{ChannelReceiver, MessageChannelReceiver},
            fragment_receiver::FragmentReceiver,
            indexed_message_reader::IndexedMessageReader,
            reliable_receiver::ReliableReceiver,
        },
        message_kinds::MessageKinds,
    },
    types::MessageIndex,
    LocalEntityAndGlobalEntityConverter, MessageContainer,
};

// Receiver Arranger Trait
pub trait ReceiverArranger: Send + Sync {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, MessageContainer)>,
        message_index: MessageIndex,
        message: MessageContainer,
    );
}

// Reliable Receiver
pub struct ReliableMessageReceiver<A: ReceiverArranger> {
    reliable_receiver: ReliableReceiver<MessageContainer>,
    incoming_messages: Vec<(MessageIndex, MessageContainer)>,
    arranger: A,
    fragment_receiver: FragmentReceiver,
}

impl<A: ReceiverArranger> ReliableMessageReceiver<A> {
    pub fn with_arranger(arranger: A) -> Self {
        Self {
            reliable_receiver: ReliableReceiver::new(),
            incoming_messages: Vec::new(),
            arranger,
            fragment_receiver: FragmentReceiver::new(),
        }
    }

    fn push_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message: MessageContainer,
    ) {
        if let Some((first_index, full_message)) =
            self.fragment_receiver
                .receive(message_kinds, converter, message)
        {
            self.arranger
                .process(&mut self.incoming_messages, first_index, full_message);
        }
    }

    pub fn buffer_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        self.reliable_receiver
            .buffer_message(message_index, message);
        let received_messages = self.reliable_receiver.receive_messages();
        for (_, received_message) in received_messages {
            self.push_message(message_kinds, converter, received_message)
        }
    }

    pub fn receive_messages(&mut self) -> Vec<(MessageIndex, MessageContainer)> {
        // return buffer
        std::mem::take(&mut self.incoming_messages)
    }
}

impl<A: ReceiverArranger> ChannelReceiver<MessageContainer> for ReliableMessageReceiver<A> {
    fn receive_messages(&mut self) -> Vec<MessageContainer> {
        self.receive_messages()
            .drain(..)
            .map(|(_, message)| message)
            .collect()
    }
}

impl<A: ReceiverArranger> MessageChannelReceiver for ReliableMessageReceiver<A> {
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        let id_w_msgs = IndexedMessageReader::read_messages(message_kinds, converter, reader)?;
        for (id, message) in id_w_msgs {
            self.buffer_message(message_kinds, converter, id, message);
        }
        Ok(())
    }
}
