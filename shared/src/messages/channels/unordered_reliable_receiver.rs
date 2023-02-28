use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    types::MessageIndex,
    MessageContainer,
};

pub type UnorderedReliableReceiver = ReliableReceiver<UnorderedArranger>;

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
