use crate::{
    messages::channels::reliable_receiver::{ReceiverArranger, ReliableReceiver},
    types::MessageIndex,
    Message,
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
        incoming_messages: &mut Vec<(MessageIndex, Box<dyn Message>)>,
        message_index: MessageIndex,
        message: Box<dyn Message>,
    ) {
        incoming_messages.push((message_index, message));
    }
}
