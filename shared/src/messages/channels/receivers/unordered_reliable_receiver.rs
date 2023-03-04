use crate::{
    messages::channels::receivers::reliable_message_receiver::{ReceiverArranger, ReliableMessageReceiver},
    types::MessageIndex,
    MessageContainer,
};

pub type UnorderedReliableReceiver = ReliableMessageReceiver<UnorderedArranger>;

impl UnorderedReliableReceiver {
    pub fn new() -> Self {
        Self::with_arranger(UnorderedArranger)
    }
}

// UnorderedArranger
pub struct UnorderedArranger;

impl ReceiverArranger for UnorderedArranger {
    fn process(
        &mut self,
        incoming_messages: &mut Vec<(MessageIndex, MessageContainer)>,
        message_index: MessageIndex,
        message: MessageContainer,
    ) {
        incoming_messages.push((message_index, message));
    }
}
